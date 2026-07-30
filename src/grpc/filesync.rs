use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, OnceLock},
};

use bollard_buildkit_proto::{
    fsutil::types::{packet::PacketType, Packet, Stat},
    moby::filesync::packet::file_sync_server::FileSync,
};
use futures_core::Stream;
use futures_util::{stream, StreamExt};
use tonic::{Request, Response, Status, Streaming};

#[cfg(unix)]
use cap_std::fs::MetadataExt as CapMetadataExt;

const CHUNK_SIZE: usize = 32 * 1024;
const MAX_ENTRIES: usize = 100_000;
const MAX_PENDING_REQUESTS: usize = 64;
const MAX_PATH_LENGTH: usize = 4096;
const MAX_LINKNAME_LENGTH: usize = 4096;

#[derive(Clone, Debug)]
pub(crate) struct FileSyncImpl {
    mounts: HashMap<String, Arc<cap_std::fs::Dir>>,
}

impl FileSyncImpl {
    pub(crate) fn new(mounts: HashMap<String, Arc<cap_std::fs::Dir>>) -> Self {
        Self { mounts }
    }
}

#[derive(Debug)]
struct SourceEntry {
    stat: Stat,
    relative: PathBuf,
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

fn lookup_mount(
    mounts: &HashMap<String, Arc<cap_std::fs::Dir>>,
    name: &str,
) -> Result<Arc<cap_std::fs::Dir>, Status> {
    mounts
        .get(name)
        .cloned()
        .ok_or_else(|| Status::not_found(format!("no access allowed to dir {name:?}")))
}

fn transfer_stream<S>(
    root: Arc<cap_std::fs::Dir>,
    input: S,
) -> Pin<Box<dyn Stream<Item = Result<Packet, Status>> + Send>>
where
    S: Stream<Item = Result<Packet, Status>> + Send + 'static,
{
    let mut input = Box::pin(input);
    let output = async_stream::try_stream! {
        let entries = collect_entries(Arc::clone(&root)).await?;
        let mut files = HashMap::new();

        for (id, entry) in entries.into_iter().enumerate() {
            let id = u32::try_from(id)
                .map_err(|_| Status::resource_exhausted("too many local source entries"))?;
            if entry.regular {
                files.insert(id, entry.relative);
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

        let mut pending = VecDeque::new();
        loop {
            let packet = match pending.pop_front() {
                Some(packet) => packet,
                None => input.next().await.ok_or_else(|| {
                    Status::failed_precondition("file sync stream ended before PACKET_FIN")
                })??,
            };
            let packet_type = PacketType::try_from(packet.r#type)
                .map_err(|_| Status::invalid_argument("unknown FileSync packet type"))?;
            match packet_type {
                PacketType::PacketReq => {
                    let relative = files.remove(&packet.id).ok_or_else(|| {
                        Status::invalid_argument(format!(
                            "invalid or repeated file request {}",
                            packet.id
                        ))
                    })?;
                    let mut file = open_requested_file(Arc::clone(&root), relative).await?;
                    let mut buffer = vec![0_u8; CHUNK_SIZE];
                    let mut finish = false;

                    loop {
                        let (incoming, read) = tokio::select! {
                            incoming = input.next() => (Some(incoming), None),
                            read = tokio::io::AsyncReadExt::read(&mut file, &mut buffer) => (None, Some(read)),
                        };

                        if let Some(incoming) = incoming {
                            let incoming = incoming.ok_or_else(|| {
                                Status::failed_precondition(
                                    "file sync stream ended during file transfer",
                                )
                            })??;
                            let incoming_type = match PacketType::try_from(incoming.r#type) {
                                Ok(packet_type) => packet_type,
                                Err(_) => Err::<PacketType, Status>(Status::invalid_argument(
                                    "unknown FileSync packet type",
                                ))?,
                            };
                            match incoming_type {
                                PacketType::PacketReq => {
                                    if files.remove(&incoming.id).is_none() {
                                        Err::<(), Status>(Status::invalid_argument(format!(
                                            "invalid or repeated file request {}",
                                            incoming.id
                                        )))?;
                                    }
                                    if pending.len() >= MAX_PENDING_REQUESTS {
                                        Err::<(), Status>(Status::resource_exhausted(
                                            "too many pending FileSync requests",
                                        ))?;
                                    }
                                    pending.push_back(incoming);
                                }
                                PacketType::PacketFin => {
                                    finish = true;
                                    break;
                                }
                                PacketType::PacketErr => {
                                    Err::<(), Status>(Status::aborted(
                                        "BuildKit aborted the FileSync transfer",
                                    ))?;
                                }
                                PacketType::PacketStat | PacketType::PacketData => {
                                    Err::<(), Status>(Status::invalid_argument(
                                        "unexpected packet type during file transfer",
                                    ))?;
                                }
                            }
                            continue;
                        }

                        let read = read.expect("FileSync read event is present").map_err(|error| {
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

                    if finish {
                        yield Packet {
                            r#type: PacketType::PacketFin as i32,
                            stat: None,
                            id: 0,
                            data: Vec::new(),
                        };
                        break;
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

async fn open_requested_file(
    root: Arc<cap_std::fs::Dir>,
    relative: PathBuf,
) -> Result<tokio::fs::File, Status> {
    let permit = filesystem_work_semaphore()
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| Status::internal("filesystem worker semaphore was closed"))?;
    let file = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let file = root.open(&relative).map_err(|error| {
            Status::failed_precondition(format!("failed to open requested file: {error}"))
        })?;
        if !file
            .metadata()
            .map_err(|error| {
                Status::failed_precondition(format!("failed to stat requested file: {error}"))
            })?
            .is_file()
        {
            return Err(Status::failed_precondition(
                "requested local source entry is no longer a regular file",
            ));
        }
        Ok(file.into_std())
    })
    .await
    .map_err(|error| Status::internal(format!("FileSync filesystem worker failed: {error}")))??;
    Ok(tokio::fs::File::from_std(file))
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

async fn collect_entries(root: Arc<cap_std::fs::Dir>) -> Result<Vec<SourceEntry>, Status> {
    let permit = filesystem_work_semaphore()
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| Status::internal("filesystem worker semaphore was closed"))?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        collect_entries_blocking(root)
    })
    .await
    .map_err(|error| Status::internal(format!("FileSync filesystem worker failed: {error}")))?
}

fn collect_entries_blocking(root: Arc<cap_std::fs::Dir>) -> Result<Vec<SourceEntry>, Status> {
    let mut entries = Vec::new();
    let mut directories = vec![(PathBuf::new(), root.try_clone().map_err(map_fs_error)?)];

    while let Some((relative_dir, directory)) = directories.pop() {
        let mut children = directory
            .entries()
            .map_err(map_fs_error)?
            .map(|entry| {
                let entry = entry.map_err(map_fs_error)?;
                let name = entry.file_name().into_string().map_err(|_| {
                    Status::invalid_argument("local source contains a non-UTF-8 filename")
                })?;
                Ok((name, entry))
            })
            .collect::<Result<Vec<_>, Status>>()?;
        children.sort_by(|left, right| left.0.cmp(&right.0));

        let mut child_directories = Vec::new();
        for (name, entry) in children {
            let relative = relative_dir.join(&name);
            let metadata = entry.metadata().map_err(map_fs_error)?;
            let source = source_entry(&directory, &name, relative.clone(), &metadata)?;
            if metadata.is_dir() {
                child_directories.push((relative, entry.open_dir().map_err(map_fs_error)?));
            }
            entries.push(source);
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
    directory: &cap_std::fs::Dir,
    name: &str,
    relative: PathBuf,
    metadata: &cap_std::fs::Metadata,
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
        (cap_file_mode_dir(), false, String::new())
    } else if file_type.is_symlink() {
        let linkname = directory
            .read_link_contents(name)
            .map_err(map_fs_error)?
            .into_os_string()
            .into_string()
            .map_err(|_| Status::invalid_argument("local source contains a non-UTF-8 symlink"))?;
        if linkname.is_empty() || linkname.len() > MAX_LINKNAME_LENGTH || linkname.contains('\0') {
            return Err(Status::invalid_argument(
                "local source contains an invalid symlink",
            ));
        }
        (cap_file_mode_symlink(), false, linkname)
    } else if file_type.is_file() {
        (0, true, String::new())
    } else {
        return Err(Status::invalid_argument(
            "local source contains an unsupported special file",
        ));
    };

    let mod_time = modification_time(metadata);
    Ok(SourceEntry {
        stat: Stat {
            path: wire_path.replace(std::path::MAIN_SEPARATOR, "/"),
            mode: mode | permissions(metadata),
            uid: uid(metadata),
            gid: gid(metadata),
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
        relative,
        regular,
    })
}

fn map_fs_error(error: std::io::Error) -> Status {
    Status::internal(format!("local source filesystem error: {error}"))
}

fn cap_file_mode_dir() -> u32 {
    super::fsutil::FileMode::Dir.bits()
}

fn cap_file_mode_symlink() -> u32 {
    super::fsutil::FileMode::Symlink.bits()
}

fn permissions(metadata: &cap_std::fs::Metadata) -> u32 {
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

fn uid(metadata: &cap_std::fs::Metadata) -> u32 {
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

fn gid(metadata: &cap_std::fs::Metadata) -> u32 {
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

fn modification_time(metadata: &cap_std::fs::Metadata) -> i64 {
    #[cfg(unix)]
    {
        metadata
            .mtime()
            .checked_mul(1_000_000_000)
            .and_then(|seconds| seconds.checked_add(metadata.mtime_nsec()))
            .unwrap_or(0)
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        0
    }
}

fn filesystem_work_semaphore() -> &'static Arc<tokio::sync::Semaphore> {
    static SEMAPHORE: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
    SEMAPHORE.get_or_init(|| Arc::new(tokio::sync::Semaphore::new(8)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;

    fn open_mount(path: &Path) -> Arc<cap_std::fs::Dir> {
        Arc::new(
            cap_std::fs::Dir::open_ambient_dir(path, cap_std::ambient_authority())
                .expect("temporary mount opens"),
        )
    }

    async fn run_protocol(root: Arc<cap_std::fs::Dir>, requests: Vec<Packet>) -> Vec<Packet> {
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
        let (sender, receiver) = mpsc::channel(8);
        sender
            .send(Packet {
                r#type: PacketType::PacketReq as i32,
                id: 0,
                ..Default::default()
            })
            .await
            .expect("request receiver is alive");
        let mut output = transfer_stream(
            open_mount(root.path()),
            ReceiverStream::new(receiver).map(Ok),
        );

        let stat = output
            .next()
            .await
            .expect("stat exists")
            .expect("stat succeeds");
        assert_eq!(stat.r#type, PacketType::PacketStat as i32);
        assert_eq!(
            stat.stat.as_ref().map(|stat| stat.path.as_str()),
            Some("hello")
        );
        let terminator = output
            .next()
            .await
            .expect("stat terminator exists")
            .expect("terminator succeeds");
        assert!(terminator.stat.is_none());
        assert_eq!(
            output
                .next()
                .await
                .expect("data exists")
                .expect("data succeeds")
                .data,
            b"world"
        );
        assert!(output
            .next()
            .await
            .expect("data terminator exists")
            .expect("data terminator succeeds")
            .data
            .is_empty());
        sender
            .send(Packet {
                r#type: PacketType::PacketFin as i32,
                ..Default::default()
            })
            .await
            .expect("request receiver is alive");
        assert_eq!(
            output
                .next()
                .await
                .expect("fin exists")
                .expect("fin succeeds")
                .r#type,
            PacketType::PacketFin as i32
        );
    }

    #[tokio::test]
    async fn rejects_unknown_mounts() {
        let root = tempdir().expect("temporary directory is created");
        let mounts = HashMap::from([(String::from("context"), open_mount(root.path()))]);
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
            open_mount(root.path()),
            ReceiverStream::new(receiver).map(Ok),
        )
        .collect::<Vec<_>>()
        .await;
        assert!(packets.iter().any(|packet| packet.is_err()));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn does_not_follow_replaced_file_outside_mount() {
        let root = tempdir().expect("temporary directory is created");
        let outside = tempdir().expect("outside directory is created");
        tokio::fs::write(root.path().join("hello"), b"safe")
            .await
            .expect("fixture is written");
        tokio::fs::write(outside.path().join("secret"), b"secret")
            .await
            .expect("outside fixture is written");

        let (sender, receiver) = mpsc::channel(8);
        let mut output = transfer_stream(
            open_mount(root.path()),
            ReceiverStream::new(receiver).map(Ok),
        );
        assert_eq!(
            output
                .next()
                .await
                .expect("stat exists")
                .expect("stat succeeds")
                .stat
                .unwrap()
                .path,
            "hello"
        );
        assert!(output
            .next()
            .await
            .expect("stat terminator exists")
            .expect("terminator succeeds")
            .stat
            .is_none());

        std::fs::remove_file(root.path().join("hello")).expect("fixture is removed");
        std::os::unix::fs::symlink(outside.path().join("secret"), root.path().join("hello"))
            .expect("outside symlink is created");
        sender
            .send(Packet {
                r#type: PacketType::PacketReq as i32,
                id: 0,
                ..Default::default()
            })
            .await
            .expect("request receiver is alive");
        let packets = output.collect::<Vec<_>>().await;
        assert!(packets.iter().any(|packet| {
            packet
                .as_ref()
                .ok()
                .is_some_and(|packet| packet.r#type == PacketType::PacketErr as i32)
        }));
        assert!(!packets.iter().any(|packet| {
            packet
                .as_ref()
                .ok()
                .is_some_and(|packet| packet.data == b"secret")
        }));
    }
}
