use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    pin::Pin,
    time::UNIX_EPOCH,
};

use bollard_buildkit_proto::{
    fsutil::types::{packet::PacketType, Packet, Stat},
    moby::filesync::packet::file_sync_server::FileSync,
};
use futures_core::Stream;
use futures_util::{stream, StreamExt};
use tonic::{Request, Response, Status, Streaming};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

const CHUNK_SIZE: usize = 32 * 1024;
const MAX_ENTRIES: usize = 100_000;
const MAX_PATH_LENGTH: usize = 4096;
const MAX_LINKNAME_LENGTH: usize = 4096;

#[derive(Clone, Debug)]
pub(crate) struct FileSyncImpl {
    mounts: HashMap<String, PathBuf>,
}

impl FileSyncImpl {
    pub(crate) fn new(mounts: HashMap<String, PathBuf>) -> Self {
        Self { mounts }
    }
}

#[derive(Debug)]
struct SourceEntry {
    stat: Stat,
    path: PathBuf,
    regular: bool,
}

#[tonic::async_trait]
impl FileSync for FileSyncImpl {
    type DiffCopyStream = Pin<Box<dyn Stream<Item = Result<Packet, Status>> + Send>>;
    type TarStreamStream = Pin<Box<dyn Stream<Item = Result<Packet, Status>> + Send>>;

    async fn diff_copy(
        &self,
        request: Request<Streaming<Packet>>,
    ) -> Result<Response<Self::DiffCopyStream>, Status> {
        let dir_name = metadata_value(request.metadata(), "dir-name")?
            .ok_or_else(|| Status::not_found("local source name is missing"))?;
        let root = lookup_mount(&self.mounts, &dir_name)?;
        validate_options(request.metadata())?;

        Ok(Response::new(transfer_stream(root, request.into_inner())))
    }

    async fn tar_stream(
        &self,
        _request: Request<Streaming<Packet>>,
    ) -> Result<Response<Self::TarStreamStream>, Status> {
        Err(Status::unimplemented("FileSync TarStream is not supported"))
    }
}

fn lookup_mount(mounts: &HashMap<String, PathBuf>, name: &str) -> Result<PathBuf, Status> {
    mounts
        .get(name)
        .cloned()
        .ok_or_else(|| Status::not_found(format!("no access allowed to dir {name:?}")))
}

fn transfer_stream<S>(
    root: PathBuf,
    input: S,
) -> Pin<Box<dyn Stream<Item = Result<Packet, Status>> + Send>>
where
    S: Stream<Item = Result<Packet, Status>> + Send + 'static,
{
    let mut input = Box::pin(input);
    let output = async_stream::try_stream! {
        let entries = collect_entries(root).await?;
        let mut files = HashMap::new();

        for (id, entry) in entries.into_iter().enumerate() {
            let id = u32::try_from(id)
                .map_err(|_| Status::resource_exhausted("too many local source entries"))?;
            if entry.regular {
                files.insert(id, entry.path.clone());
            }
            yield Packet {
                r#type: PacketType::PacketStat as i32,
                stat: Some(entry.stat),
                id: 0,
                data: Vec::new(),
            };
        }

        yield Packet {
            r#type: PacketType::PacketStat as i32,
            stat: None,
            id: 0,
            data: Vec::new(),
        };

        loop {
            let packet = input.next().await.ok_or_else(|| {
                Status::failed_precondition("file sync stream ended before PACKET_FIN")
            })??;
            let packet_type = PacketType::try_from(packet.r#type)
                .map_err(|_| Status::invalid_argument("unknown FileSync packet type"))?;
            match packet_type {
                PacketType::PacketReq => {
                    let path = files.remove(&packet.id).ok_or_else(|| {
                        Status::invalid_argument(format!(
                            "invalid or repeated file request {}",
                            packet.id
                        ))
                    })?;
                    let mut file = tokio::fs::File::open(&path).await.map_err(|error| {
                        Status::failed_precondition(format!("failed to open requested file: {error}"))
                    })?;
                    let mut buffer = vec![0_u8; CHUNK_SIZE];
                    loop {
                        let read = tokio::io::AsyncReadExt::read(&mut file, &mut buffer)
                            .await
                            .map_err(|error| {
                                Status::internal(format!("failed to read requested file: {error}"))
                            })?;
                        if read == 0 {
                            break;
                        }
                        yield Packet {
                            r#type: PacketType::PacketData as i32,
                            stat: None,
                            id: packet.id,
                            data: buffer[..read].to_vec(),
                        };
                    }
                    yield Packet {
                        r#type: PacketType::PacketData as i32,
                        stat: None,
                        id: packet.id,
                        data: Vec::new(),
                    };
                }
                PacketType::PacketFin => {
                    yield Packet {
                        r#type: PacketType::PacketFin as i32,
                        stat: None,
                        id: 0,
                        data: Vec::new(),
                    };
                    break;
                }
                PacketType::PacketErr => {
                    Err::<(), Status>(Status::aborted("BuildKit aborted the FileSync transfer"))?;
                }
                PacketType::PacketStat | PacketType::PacketData => {
                    Err::<(), Status>(Status::invalid_argument(
                        "unexpected packet type from FileSync receiver",
                    ))?;
                }
            }
        }
    };
    let output = output.flat_map(|result: Result<Packet, Status>| match result {
        Ok(packet) => stream::iter(vec![Ok(packet)]),
        Err(error) => stream::iter(vec![
            Ok(Packet {
                r#type: PacketType::PacketErr as i32,
                stat: None,
                id: 0,
                data: error.message().as_bytes().to_vec(),
            }),
            Err(error),
        ]),
    });
    Box::pin(output)
}

fn metadata_value(
    metadata: &tonic::metadata::MetadataMap,
    key: &str,
) -> Result<Option<String>, Status> {
    metadata
        .get(key)
        .map(|value| {
            value
                .to_str()
                .map(String::from)
                .map_err(|_| Status::invalid_argument(format!("invalid {key} metadata")))
        })
        .transpose()
}

fn validate_options(metadata: &tonic::metadata::MetadataMap) -> Result<(), Status> {
    for key in ["include-patterns", "exclude-patterns"] {
        if metadata.contains_key(key) {
            return Err(Status::invalid_argument(format!(
                "FileSync {key} are unsupported"
            )));
        }
    }

    for value in metadata.get_all("followpaths").iter() {
        let value = value
            .to_str()
            .map_err(|_| Status::invalid_argument("invalid followpaths metadata"))?;
        if value == "." {
            continue;
        }
        let path = Path::new(value);
        if value.is_empty()
            || value.len() > MAX_PATH_LENGTH
            || value.contains('\0')
            || value.contains('*')
            || !path
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_)))
        {
            return Err(Status::invalid_argument("unsupported FileSync follow path"));
        }
    }
    Ok(())
}

async fn collect_entries(root: PathBuf) -> Result<Vec<SourceEntry>, Status> {
    tokio::task::spawn_blocking(move || collect_entries_blocking(&root))
        .await
        .map_err(|error| Status::internal(format!("FileSync filesystem worker failed: {error}")))?
}

fn collect_entries_blocking(root: &Path) -> Result<Vec<SourceEntry>, Status> {
    let mut entries = Vec::new();
    let mut directories = vec![PathBuf::new()];

    while let Some(relative_dir) = directories.pop() {
        let directory = root.join(&relative_dir);
        let mut children = std::fs::read_dir(&directory)
            .map_err(|error| Status::internal(format!("failed to read local source: {error}")))?
            .map(|entry| {
                let entry = entry.map_err(|error| {
                    Status::internal(format!("failed to read local source entry: {error}"))
                })?;
                let name = entry.file_name().into_string().map_err(|_| {
                    Status::invalid_argument("local source contains a non-UTF-8 filename")
                })?;
                Ok((name, entry.path()))
            })
            .collect::<Result<Vec<_>, Status>>()?;
        children.sort_by(|left, right| left.0.cmp(&right.0));

        let mut child_directories = Vec::new();
        for (name, path) in children {
            let relative = relative_dir.join(&name);
            let metadata = std::fs::symlink_metadata(&path).map_err(|error| {
                Status::internal(format!("failed to stat local source entry: {error}"))
            })?;
            let entry = source_entry(relative.clone(), path, metadata)?;
            if entry.stat.mode & super::fsutil::FileMode::Dir.bits() != 0 {
                child_directories.push(relative);
            }
            entries.push(entry);
            if entries.len() > MAX_ENTRIES {
                return Err(Status::resource_exhausted(
                    "local source has too many entries",
                ));
            }
        }

        for directory in child_directories.into_iter().rev() {
            directories.push(directory);
        }
    }

    Ok(entries)
}

fn source_entry(
    relative: PathBuf,
    path: PathBuf,
    metadata: std::fs::Metadata,
) -> Result<SourceEntry, Status> {
    let wire_path = relative
        .to_str()
        .ok_or_else(|| Status::invalid_argument("local source contains a non-UTF-8 path"))?;
    if wire_path.is_empty()
        || wire_path.len() > MAX_PATH_LENGTH
        || !relative
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        return Err(Status::invalid_argument(
            "local source contains an invalid path",
        ));
    }

    let file_type = metadata.file_type();
    let (mode, regular, linkname) = if file_type.is_dir() {
        (super::fsutil::FileMode::Dir.bits(), false, String::new())
    } else if file_type.is_symlink() {
        let linkname = std::fs::read_link(&path)
            .map_err(|error| Status::internal(format!("failed to read symlink: {error}")))?
            .into_os_string()
            .into_string()
            .map_err(|_| Status::invalid_argument("local source contains a non-UTF-8 symlink"))?;
        if linkname.is_empty() || linkname.len() > MAX_LINKNAME_LENGTH || linkname.contains('\0') {
            return Err(Status::invalid_argument(
                "local source contains an invalid symlink",
            ));
        }
        (super::fsutil::FileMode::Symlink.bits(), false, linkname)
    } else if file_type.is_file() {
        (0, true, String::new())
    } else {
        return Err(Status::invalid_argument(
            "local source contains an unsupported special file",
        ));
    };

    let permissions = permissions(&metadata);
    let mod_time = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| i64::try_from(duration.as_nanos()).ok())
        .unwrap_or(0);

    Ok(SourceEntry {
        stat: Stat {
            path: wire_path.replace(std::path::MAIN_SEPARATOR, "/"),
            mode: mode | permissions,
            uid: uid(&metadata),
            gid: gid(&metadata),
            size: if regular {
                i64::try_from(metadata.len()).unwrap_or(i64::MAX)
            } else {
                0
            },
            mod_time,
            linkname,
            devmajor: 0,
            devminor: 0,
            xattrs: HashMap::new(),
        },
        path,
        regular,
    })
}

fn permissions(metadata: &std::fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        metadata.mode() & 0o777
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        0o755
    }
}

fn uid(metadata: &std::fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        metadata.uid()
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        0
    }
}

fn gid(metadata: &std::fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        metadata.gid()
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;

    async fn run_protocol(root: PathBuf, requests: Vec<Packet>) -> Vec<Packet> {
        let (sender, receiver) = mpsc::channel(8);
        for request in requests {
            sender
                .send(request)
                .await
                .expect("request receiver is alive");
        }
        drop(sender);
        transfer_stream(root, ReceiverStream::new(receiver).map(Ok))
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .map(|packet| packet.expect("FileSync packet succeeds"))
            .collect()
    }

    #[tokio::test]
    async fn sends_stats_and_requested_file_data() {
        let root = tempdir().expect("temporary directory is created");
        tokio::fs::write(root.path().join("hello"), b"world")
            .await
            .expect("fixture is written");

        let packets = run_protocol(
            root.path().to_path_buf(),
            vec![
                Packet {
                    r#type: PacketType::PacketReq as i32,
                    id: 0,
                    ..Default::default()
                },
                Packet {
                    r#type: PacketType::PacketFin as i32,
                    ..Default::default()
                },
            ],
        )
        .await;

        assert_eq!(packets[0].r#type, PacketType::PacketStat as i32);
        assert_eq!(
            packets[0].stat.as_ref().map(|stat| stat.path.as_str()),
            Some("hello")
        );
        assert_eq!(packets[1].r#type, PacketType::PacketStat as i32);
        assert!(packets[1].stat.is_none());
        assert_eq!(packets[2].data, b"world");
        assert!(packets[3].data.is_empty());
        assert_eq!(packets[4].r#type, PacketType::PacketFin as i32);
    }

    #[tokio::test]
    async fn rejects_unknown_mounts() {
        let root = tempdir().expect("temporary directory is created");
        let mounts = HashMap::from([(String::from("context"), root.path().to_owned())]);
        let error = lookup_mount(&mounts, "missing").expect_err("unknown mount must fail");
        assert_eq!(error.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn rejects_repeated_file_requests() {
        let root = tempdir().expect("temporary directory is created");
        tokio::fs::write(root.path().join("hello"), b"world")
            .await
            .expect("fixture is written");
        let (sender, receiver) = mpsc::channel(8);
        sender
            .send(Packet {
                r#type: PacketType::PacketReq as i32,
                id: 0,
                ..Default::default()
            })
            .await
            .expect("request receiver is alive");
        sender
            .send(Packet {
                r#type: PacketType::PacketReq as i32,
                id: 0,
                ..Default::default()
            })
            .await
            .expect("request receiver is alive");
        let packets = transfer_stream(
            root.path().to_owned(),
            ReceiverStream::new(receiver).map(Ok),
        )
        .collect::<Vec<_>>()
        .await;
        assert!(packets.iter().any(|packet| packet.is_err()));
    }
}
