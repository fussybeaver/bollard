use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
};

use bollard_buildkit_proto::{
    fsutil::types::{packet::PacketType, Packet, Stat},
    moby::filesync::v1::file_sync_server::FileSync,
};
use futures_core::Stream;
use futures_util::StreamExt;
use tonic::{metadata::MetadataMap, Request, Response, Status, Streaming};

#[cfg(unix)]
use cap_std::fs::MetadataExt;
#[cfg(not(unix))]
use std::time::UNIX_EPOCH;

const DIR_NAME_METADATA: &str = "dir-name";
const INCLUDE_PATTERNS_METADATA: &str = "include-patterns";
const EXCLUDE_PATTERNS_METADATA: &str = "exclude-patterns";
const FOLLOW_PATHS_METADATA: &str = "followpaths";
const MAX_ENTRIES: usize = 100_000;
const MAX_PATH_LENGTH: usize = 4096;
const MAX_LINKNAME_LENGTH: usize = 4096;
const ENTRY_QUEUE_CAPACITY: usize = 128;

#[cfg(test)]
use bollard_buildkit_proto::moby::filesync::v1::file_sync_server::FileSyncServer;

#[derive(Clone)]
pub(crate) struct FileSyncImpl {
    mounts: HashMap<String, Arc<cap_std::fs::Dir>>,
}

#[derive(Debug)]
struct SourceEntry {
    stat: Stat,
    position: u32,
    regular: bool,
    relative: PathBuf,
}

struct ScanFrame {
    relative: PathBuf,
    directory: cap_std::fs::Dir,
    names: Vec<OsString>,
    next_name: usize,
}

enum SessionEvent {
    Entry(Option<Result<SourceEntry, Status>>),
    Packet(Option<Result<Packet, Status>>),
}

impl FileSyncImpl {
    pub(crate) fn new(mounts: HashMap<String, Arc<cap_std::fs::Dir>>) -> Self {
        Self { mounts }
    }
}

impl std::fmt::Debug for FileSyncImpl {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FileSyncImpl")
            .field("mounts", &self.mounts.keys().collect::<Vec<_>>())
            .finish()
    }
}

#[tonic::async_trait]
impl FileSync for FileSyncImpl {
    type DiffCopyStream = Pin<Box<dyn Stream<Item = Result<Packet, Status>> + Send>>;
    type TarStreamStream = futures_util::stream::Empty<Result<Packet, Status>>;

    async fn diff_copy(
        &self,
        request: Request<Streaming<Packet>>,
    ) -> Result<Response<Self::DiffCopyStream>, Status> {
        let name = mount_name(request.metadata())?;
        let root = lookup_mount(&self.mounts, &name)?;
        validate_options(request.metadata())?;

        let (entries_sender, mut entries_receiver) =
            tokio::sync::mpsc::channel(ENTRY_QUEUE_CAPACITY);
        let mut scanner = Some(tokio::task::spawn_blocking(move || {
            let result = scan_entries(root, entries_sender.clone());
            if let Err(error) = result {
                let _ = entries_sender.blocking_send(Err(error));
            }
        }));
        let mut input = Box::pin(request.into_inner());

        let output = async_stream::try_stream! {
            let mut enumeration_finished = false;

            loop {
                if enumeration_finished {
                    let packet = input.next().await.ok_or_else(|| {
                        Status::failed_precondition(
                            "FileSync stream ended before PACKET_FIN",
                        )
                    })??;
                    match PacketType::try_from(packet.r#type)
                        .map_err(|_| Status::invalid_argument("unknown FileSync packet type"))?
                    {
                        PacketType::PacketFin => {
                            yield fin_response_packet();
                            break;
                        }
                        PacketType::PacketErr => {
                            Err::<(), Status>(Status::aborted(
                                "BuildKit aborted the FileSync transfer",
                            ))?;
                        }
                        PacketType::PacketReq => {
                            let error = Status::unimplemented(
                                "FileSync file requests are not implemented",
                            );
                            yield error_packet(&error);
                            Err::<(), Status>(error)?;
                        }
                        PacketType::PacketStat | PacketType::PacketData => {
                            Err::<(), Status>(Status::invalid_argument(
                                "unexpected packet type from FileSync receiver",
                            ))?;
                        }
                    }
                    continue;
                }

                let event = tokio::select! {
                    event = entries_receiver.recv() => SessionEvent::Entry(event),
                    packet = input.next() => SessionEvent::Packet(packet),
                };
                match event {
                    SessionEvent::Entry(event) => match event {
                        Some(Ok(entry)) => {
                            let SourceEntry {
                                stat,
                                position,
                                regular,
                                relative,
                            } = entry;
                            let _ = (position, regular, relative);
                                yield stat_entry_packet(stat);
                        }
                        Some(Err(error)) => {
                            yield error_packet(&error);
                            Err::<(), Status>(error)?;
                        }
                        None => {
                            if let Some(scanner) = scanner.take() {
                                if let Err(join_error) = scanner.await {
                                    let error = Status::internal(format!(
                                        "FileSync scanner task failed: {join_error}"
                                    ));
                                    yield error_packet(&error);
                                    Err::<(), Status>(error)?;
                                }
                            }
                            yield stat_terminator();
                            enumeration_finished = true;
                        }
                    },
                    SessionEvent::Packet(packet) => {
                        let packet = packet.ok_or_else(|| {
                            Status::failed_precondition(
                                "FileSync stream ended before PACKET_FIN",
                            )
                        })??;
                        let packet_type = PacketType::try_from(packet.r#type)
                            .map_err(|_| Status::invalid_argument("unknown FileSync packet type"))?;
                        match packet_type {
                            PacketType::PacketErr => {
                                Err::<(), Status>(Status::aborted(
                                    "BuildKit aborted the FileSync transfer",
                                ))?;
                            }
                            PacketType::PacketReq => {
                                let error = Status::unimplemented(
                                    "FileSync file requests are not implemented",
                                );
                                yield error_packet(&error);
                                Err::<(), Status>(error)?;
                            }
                            PacketType::PacketFin => {
                                Err::<(), Status>(Status::failed_precondition(
                                    "FileSync received PACKET_FIN before STAT termination",
                                ))?;
                            }
                            PacketType::PacketStat | PacketType::PacketData => {
                                Err::<(), Status>(Status::invalid_argument(
                                    "unexpected packet type from FileSync receiver",
                                ))?;
                            }
                        }
                    }
                }
            }
        };

        Ok(Response::new(Box::pin(output)))
    }

    async fn tar_stream(
        &self,
        _request: Request<Streaming<Packet>>,
    ) -> Result<Response<Self::TarStreamStream>, Status> {
        Err(Status::unimplemented(
            "FileSync TarStream is not implemented",
        ))
    }
}

fn mount_name(metadata: &MetadataMap) -> Result<String, Status> {
    let value = metadata
        .get(DIR_NAME_METADATA)
        .ok_or_else(|| Status::not_found("local source name is missing"))?;
    value
        .to_str()
        .map(str::to_owned)
        .map_err(|_| Status::invalid_argument("invalid dir-name metadata"))
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

fn validate_options(metadata: &MetadataMap) -> Result<(), Status> {
    for key in [INCLUDE_PATTERNS_METADATA, EXCLUDE_PATTERNS_METADATA] {
        if metadata.contains_key(key) || metadata.contains_key(format!("{key}-encoded")) {
            return Err(Status::invalid_argument(format!(
                "FileSync {key} are unsupported"
            )));
        }
    }

    if metadata.contains_key(format!("{FOLLOW_PATHS_METADATA}-encoded")) {
        return Err(Status::invalid_argument(
            "encoded FileSync followpaths are unsupported",
        ));
    }
    for value in metadata.get_all(FOLLOW_PATHS_METADATA).iter() {
        let value = value
            .to_str()
            .map_err(|_| Status::invalid_argument("invalid followpaths metadata"))?;
        if value != "." {
            return Err(Status::unimplemented(
                "literal FileSync followpaths are not implemented",
            ));
        }
    }
    Ok(())
}

fn scan_entries(
    root: Arc<cap_std::fs::Dir>,
    sender: tokio::sync::mpsc::Sender<Result<SourceEntry, Status>>,
) -> Result<(), Status> {
    let root = root.try_clone().map_err(|error| {
        Status::internal(format!("failed to retain local source root: {error}"))
    })?;
    let names = sorted_names(&root)?;
    let mut frames = vec![ScanFrame {
        relative: PathBuf::new(),
        directory: root,
        names,
        next_name: 0,
    }];
    let mut position = 0_u32;

    while let Some(mut frame) = frames.pop() {
        let Some(name) = frame.names.get(frame.next_name).cloned() else {
            continue;
        };
        frame.next_name += 1;

        let relative = frame.relative.join(&name);
        let metadata = frame
            .directory
            .symlink_metadata(&name)
            .map_err(|error| filesystem_error("stat", &relative, error))?;
        let (stat, regular) = source_stat(&frame.directory, &name, &relative, &metadata)?;
        if position as usize >= MAX_ENTRIES {
            return Err(Status::resource_exhausted(
                "local source has too many entries",
            ));
        }
        let current_position = position;
        position = position
            .checked_add(1)
            .ok_or_else(|| Status::resource_exhausted("local source entry ID exhausted"))?;

        if sender
            .blocking_send(Ok(SourceEntry {
                stat,
                position: current_position,
                regular,
                relative: relative.clone(),
            }))
            .is_err()
        {
            return Ok(());
        }

        let file_type = metadata.file_type();
        let child = if file_type.is_dir() {
            let directory = frame
                .directory
                .open_dir(&name)
                .map_err(|error| filesystem_error("open directory", &relative, error))?;
            Some(ScanFrame {
                relative,
                names: sorted_names(&directory)?,
                directory,
                next_name: 0,
            })
        } else {
            None
        };

        frames.push(frame);
        if let Some(child) = child {
            frames.push(child);
        }
    }

    Ok(())
}

fn sorted_names(directory: &cap_std::fs::Dir) -> Result<Vec<OsString>, Status> {
    let mut names = directory
        .entries()
        .map_err(|error| {
            Status::internal(format!("failed to read local source directory: {error}"))
        })?
        .map(|entry| {
            entry.map(|entry| entry.file_name()).map_err(|error| {
                Status::internal(format!("failed to read local source entry: {error}"))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    names.sort_unstable();
    Ok(names)
}

fn source_stat(
    directory: &cap_std::fs::Dir,
    name: &OsStr,
    relative: &Path,
    metadata: &cap_std::fs::Metadata,
) -> Result<(Stat, bool), Status> {
    let path = relative
        .to_str()
        .ok_or_else(|| Status::invalid_argument("local source contains a non-UTF-8 path"))?;
    if path.is_empty()
        || path.len() > MAX_PATH_LENGTH
        || !relative
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        return Err(Status::invalid_argument(
            "local source contains an invalid path",
        ));
    }

    let file_type = metadata.file_type();
    let (mode, size, linkname, regular) = if file_type.is_dir() {
        (file_mode_dir(metadata), 0, String::new(), false)
    } else if file_type.is_symlink() {
        let linkname = directory
            .read_link_contents(name)
            .map_err(|error| filesystem_error("read symlink", relative, error))?
            .into_os_string()
            .into_string()
            .map_err(|_| Status::invalid_argument("local source contains a non-UTF-8 symlink"))?;
        if linkname.is_empty() || linkname.len() > MAX_LINKNAME_LENGTH || linkname.contains('\0') {
            return Err(Status::invalid_argument(
                "local source contains an invalid symlink",
            ));
        }
        (
            super::fsutil::FileMode::Symlink.bits() | 0o777,
            0,
            linkname.replace(std::path::MAIN_SEPARATOR, "/"),
            false,
        )
    } else if file_type.is_file() {
        let size = i64::try_from(metadata.len())
            .map_err(|_| Status::resource_exhausted("local source file is too large"))?;
        (file_permissions(metadata), size, String::new(), true)
    } else {
        return Err(Status::invalid_argument(format!(
            "local source contains an unsupported special file: {path:?}"
        )));
    };

    Ok((
        Stat {
            path: path.replace(std::path::MAIN_SEPARATOR, "/"),
            mode,
            uid: file_uid(metadata),
            gid: file_gid(metadata),
            size,
            mod_time: modification_time(metadata),
            linkname,
            ..Default::default()
        },
        regular,
    ))
}

fn filesystem_error(operation: &str, path: &Path, error: std::io::Error) -> Status {
    Status::internal(format!(
        "failed to {operation} local source {path:?}: {error}"
    ))
}

fn file_mode_dir(metadata: &cap_std::fs::Metadata) -> u32 {
    super::fsutil::FileMode::Dir.bits() | file_permissions(metadata)
}

#[cfg(unix)]
fn file_permissions(metadata: &cap_std::fs::Metadata) -> u32 {
    metadata.mode() & 0o777
}

#[cfg(not(unix))]
fn file_permissions(metadata: &cap_std::fs::Metadata) -> u32 {
    let _ = metadata;
    0o755
}

#[cfg(unix)]
fn file_uid(metadata: &cap_std::fs::Metadata) -> u32 {
    metadata.uid()
}

#[cfg(not(unix))]
fn file_uid(metadata: &cap_std::fs::Metadata) -> u32 {
    let _ = metadata;
    0
}

#[cfg(unix)]
fn file_gid(metadata: &cap_std::fs::Metadata) -> u32 {
    metadata.gid()
}

#[cfg(not(unix))]
fn file_gid(metadata: &cap_std::fs::Metadata) -> u32 {
    let _ = metadata;
    0
}

#[cfg(unix)]
fn modification_time(metadata: &cap_std::fs::Metadata) -> i64 {
    metadata
        .mtime()
        .checked_mul(1_000_000_000)
        .and_then(|seconds| seconds.checked_add(metadata.mtime_nsec()))
        .unwrap_or(0)
}

#[cfg(not(unix))]
fn modification_time(metadata: &cap_std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| i64::try_from(duration.as_nanos()).ok())
        .unwrap_or(0)
}

fn stat_entry_packet(stat: Stat) -> Packet {
    Packet {
        r#type: PacketType::PacketStat as i32,
        stat: Some(stat),
        ..Default::default()
    }
}

fn stat_terminator() -> Packet {
    Packet {
        r#type: PacketType::PacketStat as i32,
        ..Default::default()
    }
}

fn fin_response_packet() -> Packet {
    Packet {
        r#type: PacketType::PacketFin as i32,
        ..Default::default()
    }
}

fn error_packet(error: &Status) -> Packet {
    Packet {
        r#type: PacketType::PacketErr as i32,
        data: error.message().as_bytes().to_vec(),
        ..Default::default()
    }
}

#[cfg(test)]
use futures_util::stream;
#[cfg(test)]
use std::collections::HashSet;
#[cfg(test)]
use tokio::sync::mpsc;
#[cfg(test)]
use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
#[cfg(test)]
use tonic::transport::Server;

#[cfg(test)]
pub(crate) const FILE_JOB_QUEUE_CAPACITY: usize = 128;
#[cfg(test)]
pub(crate) const OUTPUT_QUEUE_CAPACITY: usize = 16;
#[cfg(test)]
pub(crate) const FILE_WORKER_COUNT: usize = 4;
#[cfg(test)]
pub(crate) const FILE_READ_BUFFER_SIZE: usize = 32 * 1024;

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ContractEntry {
    pub(crate) path: &'static str,
    pub(crate) regular: bool,
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct PacketContract {
    entries: Vec<ContractEntry>,
}

#[cfg(test)]
impl PacketContract {
    pub(crate) fn new(entries: impl IntoIterator<Item = ContractEntry>) -> Self {
        Self {
            entries: entries.into_iter().collect(),
        }
    }

    pub(crate) fn stat_packets(&self) -> Vec<Packet> {
        self.entries
            .iter()
            .map(|entry| stat_packet(entry.path))
            .chain(std::iter::once(stat_packet_without_entry()))
            .collect()
    }

    pub(crate) fn validate_stat_packets(&self, packets: &[Packet]) -> Result<(), String> {
        if packets.len() != self.entries.len() + 1 {
            return Err(format!(
                "expected {} STAT packets, got {}",
                self.entries.len() + 1,
                packets.len()
            ));
        }

        for (index, (packet, expected)) in packets.iter().zip(self.entries.iter()).enumerate() {
            if packet.r#type != PacketType::PacketStat as i32 {
                return Err(format!("packet {index} is not PACKET_STAT"));
            }
            if packet.id != 0 {
                return Err(format!("packet {index} has non-zero STAT ID"));
            }
            if packet.stat.as_ref().map(|stat| stat.path.as_str()) != Some(expected.path) {
                return Err(format!("packet {index} has the wrong path"));
            }
        }

        let terminator = packets.last().expect("length checked above");
        if terminator.r#type != PacketType::PacketStat as i32
            || terminator.id != 0
            || terminator.stat.is_some()
        {
            return Err(String::from("STAT terminator is malformed"));
        }
        Ok(())
    }

    pub(crate) fn regular_ids(&self) -> HashSet<u32> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| entry.regular.then_some(index as u32))
            .collect()
    }
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct RequestLedger {
    available: HashSet<u32>,
}

#[cfg(test)]
impl RequestLedger {
    pub(crate) fn new(contract: &PacketContract) -> Self {
        Self {
            available: contract.regular_ids(),
        }
    }

    pub(crate) fn accept(&mut self, id: u32) -> Result<(), String> {
        if self.available.remove(&id) {
            Ok(())
        } else {
            Err(format!("invalid or repeated file request {id}"))
        }
    }
}

#[cfg(test)]
pub(crate) fn stat_packet(path: &'static str) -> Packet {
    Packet {
        r#type: PacketType::PacketStat as i32,
        stat: Some(Stat {
            path: path.to_owned(),
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[cfg(test)]
pub(crate) fn stat_packet_without_entry() -> Packet {
    Packet {
        r#type: PacketType::PacketStat as i32,
        ..Default::default()
    }
}

#[cfg(test)]
pub(crate) fn request_packet(id: u32) -> Packet {
    Packet {
        r#type: PacketType::PacketReq as i32,
        id,
        ..Default::default()
    }
}

#[cfg(test)]
pub(crate) fn data_packet(id: u32, data: impl Into<Vec<u8>>) -> Packet {
    Packet {
        r#type: PacketType::PacketData as i32,
        id,
        data: data.into(),
        ..Default::default()
    }
}

#[cfg(test)]
pub(crate) fn fin_packet() -> Packet {
    Packet {
        r#type: PacketType::PacketFin as i32,
        ..Default::default()
    }
}

#[cfg(test)]
pub(crate) fn err_packet(message: impl Into<Vec<u8>>) -> Packet {
    Packet {
        r#type: PacketType::PacketErr as i32,
        data: message.into(),
        ..Default::default()
    }
}

#[cfg(test)]
pub(crate) fn collect_file_data(packets: &[Packet], id: u32) -> Result<Vec<u8>, String> {
    let mut data = Vec::new();
    let mut terminated = false;

    for packet in packets.iter().filter(|packet| packet.id == id) {
        if packet.r#type != PacketType::PacketData as i32 {
            return Err(format!("file {id} contains a non-DATA packet"));
        }
        if packet.data.is_empty() {
            if terminated {
                return Err(format!("file {id} has repeated EOF"));
            }
            terminated = true;
        } else if terminated {
            return Err(format!("file {id} has DATA after EOF"));
        } else {
            data.extend_from_slice(&packet.data);
        }
    }

    if terminated {
        Ok(data)
    } else {
        Err(format!("file {id} has no DATA EOF"))
    }
}

#[cfg(test)]
pub(crate) struct ScriptedPeer {
    requests: mpsc::Sender<Packet>,
    responses: Pin<Box<dyn Stream<Item = Result<Packet, Status>> + Send>>,
}

#[cfg(test)]
impl ScriptedPeer {
    pub(crate) fn new(
        responses: impl Stream<Item = Result<Packet, Status>> + Send + 'static,
    ) -> (Self, ReceiverStream<Packet>) {
        let (requests, receiver) = mpsc::channel(OUTPUT_QUEUE_CAPACITY);
        (
            Self {
                requests,
                responses: Box::pin(responses),
            },
            ReceiverStream::new(receiver),
        )
    }

    pub(crate) async fn send(&self, packet: Packet) -> Result<(), String> {
        self.requests
            .send(packet)
            .await
            .map_err(|_| String::from("scripted peer request channel closed"))
    }

    pub(crate) async fn next(&mut self) -> Option<Result<Packet, Status>> {
        futures_util::StreamExt::next(&mut self.responses).await
    }

    pub(crate) fn close(self) {
        drop(self.requests);
        drop(self.responses);
    }
}

#[cfg(test)]
#[derive(Debug, Default)]
struct UnimplementedFileSync;

#[cfg(test)]
#[tonic::async_trait]
impl FileSync for UnimplementedFileSync {
    type DiffCopyStream = stream::Empty<Result<Packet, Status>>;
    type TarStreamStream = stream::Empty<Result<Packet, Status>>;

    async fn diff_copy(
        &self,
        _request: Request<Streaming<Packet>>,
    ) -> Result<Response<Self::DiffCopyStream>, Status> {
        Err(Status::unimplemented("FileSync sender is not implemented"))
    }

    async fn tar_stream(
        &self,
        _request: Request<Streaming<Packet>>,
    ) -> Result<Response<Self::TarStreamStream>, Status> {
        Err(Status::unimplemented(
            "FileSync TarStream is not implemented",
        ))
    }
}

#[cfg(test)]
async fn start_unimplemented_server() -> (
    std::net::SocketAddr,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("FileSync test listener binds");
    let address = listener.local_addr().expect("FileSync test address exists");
    let (shutdown_sender, shutdown_receiver) = tokio::sync::oneshot::channel();
    let server = Server::builder()
        .add_service(FileSyncServer::new(UnimplementedFileSync))
        .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async move {
            let _ = shutdown_receiver.await;
        });
    let server_task = tokio::spawn(async move {
        let _ = server.await;
    });
    (address, shutdown_sender, server_task)
}

#[cfg(test)]
async fn red_baseline_stream() -> Result<tonic::Streaming<Packet>, Status> {
    let (address, shutdown_sender, server_task) = start_unimplemented_server().await;
    let mut client =
        bollard_buildkit_proto::moby::filesync::v1::file_sync_client::FileSyncClient::connect(
            format!("http://{address}"),
        )
        .await
        .map_err(|error| Status::unknown(error.to_string()))?;
    let (_peer, requests) = ScriptedPeer::new(stream::empty());
    let result = client
        .diff_copy(requests)
        .await
        .map(|response| response.into_inner());
    let _ = shutdown_sender.send(());
    let _ = server_task.await;
    result
}

#[cfg(test)]
async fn start_filesync_server(
    root: Arc<cap_std::fs::Dir>,
) -> (
    std::net::SocketAddr,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("FileSync test listener binds");
    let address = listener.local_addr().expect("FileSync test address exists");
    let mounts = HashMap::from([(String::from("context"), root)]);
    let (shutdown_sender, shutdown_receiver) = tokio::sync::oneshot::channel();
    let server = Server::builder()
        .add_service(FileSyncServer::new(FileSyncImpl::new(mounts)))
        .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async move {
            let _ = shutdown_receiver.await;
        });
    let server_task = tokio::spawn(async move {
        let _ = server.await;
    });
    (address, shutdown_sender, server_task)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use tempfile::tempdir;

    fn open_mount(path: &Path) -> Arc<cap_std::fs::Dir> {
        Arc::new(
            cap_std::fs::Dir::open_ambient_dir(path, cap_std::ambient_authority())
                .expect("temporary mount opens"),
        )
    }

    async fn scan_fixture(root: Arc<cap_std::fs::Dir>) -> Vec<SourceEntry> {
        let (sender, mut receiver) = mpsc::channel(ENTRY_QUEUE_CAPACITY);
        let scanner = tokio::task::spawn_blocking(move || scan_entries(root, sender));
        let mut entries = Vec::new();
        while let Some(event) = receiver.recv().await {
            entries.push(event.expect("fixture scan succeeds"));
        }
        scanner
            .await
            .expect("fixture scanner joins")
            .expect("fixture scanner succeeds");
        entries
    }

    fn sample_contract() -> PacketContract {
        PacketContract::new([
            ContractEntry {
                path: "directory",
                regular: false,
            },
            ContractEntry {
                path: "directory/input.txt",
                regular: true,
            },
            ContractEntry {
                path: "link",
                regular: false,
            },
            ContractEntry {
                path: "empty.txt",
                regular: true,
            },
        ])
    }

    #[test]
    fn contract_tracks_positional_regular_file_ids() {
        let contract = sample_contract();
        contract
            .validate_stat_packets(&contract.stat_packets())
            .expect("fixture STAT packets are valid");
        assert_eq!(contract.regular_ids(), HashSet::from([1, 3]));
    }

    #[test]
    fn contract_requires_zero_stat_packet_ids() {
        let contract = sample_contract();
        let mut packets = contract.stat_packets();
        packets[1].id = 1;
        assert!(contract.validate_stat_packets(&packets).is_err());
    }

    #[test]
    fn request_ledger_rejects_duplicate_unknown_and_non_regular_ids() {
        let contract = sample_contract();
        let mut ledger = RequestLedger::new(&contract);
        assert!(ledger.accept(1).is_ok());
        assert!(ledger.accept(1).is_err());
        assert!(ledger.accept(0).is_err());
        assert!(ledger.accept(99).is_err());
    }

    #[test]
    fn data_collection_requires_one_empty_eof_packet() {
        let packets = [
            data_packet(1, b"first".to_vec()),
            data_packet(3, b"other".to_vec()),
            data_packet(1, b"second".to_vec()),
            data_packet(1, Vec::new()),
            data_packet(3, Vec::new()),
        ];
        assert_eq!(collect_file_data(&packets, 1).unwrap(), b"firstsecond");
        assert_eq!(collect_file_data(&packets, 3).unwrap(), b"other");
    }

    #[test]
    fn protocol_bounds_are_fixed_before_sender_implementation() {
        assert_eq!(ENTRY_QUEUE_CAPACITY, 128);
        assert_eq!(FILE_JOB_QUEUE_CAPACITY, 128);
        assert_eq!(OUTPUT_QUEUE_CAPACITY, 16);
        assert_eq!(FILE_WORKER_COUNT, 4);
        assert_eq!(FILE_READ_BUFFER_SIZE, 32 * 1024);
    }

    #[tokio::test]
    async fn scripted_peer_uses_bounded_request_and_response_channels() {
        let (mut peer, mut requests) =
            ScriptedPeer::new(stream::iter([Ok(data_packet(1, b"response".to_vec()))]));
        peer.send(request_packet(1))
            .await
            .expect("scripted peer accepts requests");
        assert_eq!(requests.next().await, Some(request_packet(1)));
        assert_eq!(
            peer.next()
                .await
                .expect("scripted response exists")
                .unwrap(),
            data_packet(1, b"response".to_vec())
        );
        peer.close();
    }

    #[tokio::test]
    async fn scanner_walks_depth_first_lexically_and_registers_positions() {
        let root = tempdir().expect("temporary directory is created");
        std::fs::create_dir(root.path().join("b")).expect("b directory is created");
        std::fs::create_dir(root.path().join("a")).expect("a directory is created");
        std::fs::write(root.path().join("b/z.txt"), b"z").expect("z file is created");
        std::fs::write(root.path().join("a/x.txt"), b"x").expect("x file is created");
        #[cfg(unix)]
        std::os::unix::fs::symlink("a/x.txt", root.path().join("link"))
            .expect("symlink is created");

        let entries = scan_fixture(open_mount(root.path())).await;
        let paths = entries
            .iter()
            .map(|entry| entry.stat.path.as_str())
            .collect::<Vec<_>>();
        #[cfg(unix)]
        assert_eq!(paths, ["a", "a/x.txt", "b", "b/z.txt", "link"]);
        #[cfg(not(unix))]
        assert_eq!(paths, ["a", "a/x.txt", "b", "b/z.txt"]);
        assert!(entries.iter().all(|entry| entry.stat.path != "."));
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.regular)
                .map(|entry| entry.position)
                .collect::<Vec<_>>(),
            [1, 3]
        );
        assert!(entries
            .iter()
            .all(|entry| entry.stat.path.len() <= MAX_PATH_LENGTH));
    }

    #[tokio::test]
    async fn scanner_emits_regular_and_symlink_metadata() {
        let root = tempdir().expect("temporary directory is created");
        std::fs::write(root.path().join("empty"), b"").expect("empty file is created");
        #[cfg(unix)]
        std::os::unix::fs::symlink("empty", root.path().join("link")).expect("symlink is created");

        let entries = scan_fixture(open_mount(root.path())).await;
        let empty = entries
            .iter()
            .find(|entry| entry.stat.path == "empty")
            .expect("empty file stat exists");
        assert!(empty.regular);
        assert_eq!(empty.stat.size, 0);
        assert_eq!(
            empty.stat.mode & super::super::fsutil::FileMode::Type.bits(),
            0
        );

        #[cfg(unix)]
        {
            let link = entries
                .iter()
                .find(|entry| entry.stat.path == "link")
                .expect("symlink stat exists");
            assert!(!link.regular);
            assert_ne!(
                link.stat.mode & super::super::fsutil::FileMode::Symlink.bits(),
                0
            );
            assert_eq!(link.stat.linkname, "empty");
            assert_eq!(link.stat.size, 0);
        }
    }

    #[test]
    fn scanner_rejects_unsupported_options_and_long_paths() {
        let mut metadata = MetadataMap::new();
        metadata.insert(
            DIR_NAME_METADATA,
            tonic::metadata::MetadataValue::try_from("context").expect("metadata value is valid"),
        );
        assert_eq!(mount_name(&metadata).expect("mount name exists"), "context");
        assert_eq!(
            mount_name(&MetadataMap::new()).unwrap_err().code(),
            tonic::Code::NotFound
        );

        let mut metadata = MetadataMap::new();
        metadata.insert(
            INCLUDE_PATTERNS_METADATA,
            tonic::metadata::MetadataValue::try_from("*.tmp").expect("metadata value is valid"),
        );
        assert_eq!(
            validate_options(&metadata).unwrap_err().code(),
            tonic::Code::InvalidArgument
        );

        let mut metadata = MetadataMap::new();
        metadata.insert(
            FOLLOW_PATHS_METADATA,
            tonic::metadata::MetadataValue::try_from("**/*.rs").expect("metadata value is valid"),
        );
        assert_eq!(
            validate_options(&metadata).unwrap_err().code(),
            tonic::Code::Unimplemented
        );
        let mut metadata = MetadataMap::new();
        metadata.insert(
            FOLLOW_PATHS_METADATA,
            tonic::metadata::MetadataValue::try_from(".").expect("metadata value is valid"),
        );
        validate_options(&metadata).expect("the whole-tree follow path is supported");

        let root = tempdir().expect("temporary directory is created");
        let mounts = HashMap::from([(String::from("context"), open_mount(root.path()))]);
        assert_eq!(
            lookup_mount(&mounts, "missing").unwrap_err().code(),
            tonic::Code::NotFound
        );
        let file = root.path().join("file");
        std::fs::write(&file, b"data").expect("file is created");
        let metadata =
            cap_std::fs::Dir::open_ambient_dir(root.path(), cap_std::ambient_authority())
                .expect("mount opens")
                .symlink_metadata("file")
                .expect("file metadata exists");
        let long_path = PathBuf::from("x".repeat(MAX_PATH_LENGTH + 1));
        assert_eq!(
            source_stat(
                &cap_std::fs::Dir::open_ambient_dir(root.path(), cap_std::ambient_authority())
                    .expect("mount opens"),
                OsStr::new("file"),
                &long_path,
                &metadata,
            )
            .unwrap_err()
            .code(),
            tonic::Code::InvalidArgument
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn scanner_rejects_special_files() {
        use std::os::unix::net::UnixListener;

        let root = tempdir().expect("temporary directory is created");
        let _socket =
            UnixListener::bind(root.path().join("socket")).expect("unix socket is created");
        let (sender, mut receiver) = mpsc::channel(ENTRY_QUEUE_CAPACITY);
        let scanner = tokio::task::spawn_blocking({
            let root = open_mount(root.path());
            move || {
                let result = scan_entries(root, sender.clone());
                if let Err(error) = &result {
                    let _ = sender.blocking_send(Err(error.clone()));
                }
                result
            }
        });
        let event = receiver.recv().await.expect("scanner reports an error");
        assert_eq!(event.unwrap_err().code(), tonic::Code::InvalidArgument);
        assert!(scanner.await.expect("scanner joins").is_err());
    }

    #[tokio::test]
    async fn diff_copy_streams_stats_and_fails_closed_on_requests() {
        let root = tempdir().expect("temporary directory is created");
        std::fs::write(root.path().join("input"), b"source").expect("source file is created");
        let (address, shutdown_sender, server_task) =
            start_filesync_server(open_mount(root.path())).await;
        let mut client =
            bollard_buildkit_proto::moby::filesync::v1::file_sync_client::FileSyncClient::connect(
                format!("http://{address}"),
            )
            .await
            .expect("FileSync client connects");
        let (sender, receiver) = mpsc::channel(8);
        let mut request = Request::new(ReceiverStream::new(receiver));
        request.metadata_mut().insert(
            DIR_NAME_METADATA,
            tonic::metadata::MetadataValue::try_from("context").expect("metadata value is valid"),
        );
        let mut responses = client
            .diff_copy(request)
            .await
            .expect("DiffCopy starts")
            .into_inner();
        let stat = responses
            .message()
            .await
            .expect("STAT response succeeds")
            .expect("STAT response exists");
        assert_eq!(stat.r#type, PacketType::PacketStat as i32);
        assert_eq!(stat.id, 0);
        assert_eq!(stat.stat.expect("entry stat exists").path, "input");
        let terminator = responses
            .message()
            .await
            .expect("STAT terminator succeeds")
            .expect("STAT terminator exists");
        assert_eq!(terminator.r#type, PacketType::PacketStat as i32);
        assert!(terminator.stat.is_none());

        sender
            .send(request_packet(0))
            .await
            .expect("request channel remains open");
        let error_packet = responses
            .message()
            .await
            .expect("protocol error packet succeeds")
            .expect("protocol error packet exists");
        assert_eq!(error_packet.r#type, PacketType::PacketErr as i32);
        let error = responses
            .message()
            .await
            .expect_err("REQ ends the FileSync stream");
        assert_eq!(error.code(), tonic::Code::Unimplemented);

        drop(sender);
        let _ = shutdown_sender.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    #[ignore = "removed when the FileSync coordinator is implemented"]
    async fn diff_copy_red_baseline_accepts_pipelined_requests() {
        let _ = request_packet(1);
        let _ = request_packet(3);
        let _ = red_baseline_stream()
            .await
            .expect("FileSync sender accepts pipelined requests");
    }

    #[tokio::test]
    #[ignore = "removed when the FileSync coordinator is implemented"]
    async fn diff_copy_red_baseline_reports_protocol_errors() {
        let _ = err_packet("peer failure");
        let _ = fin_packet();
        let _ = red_baseline_stream()
            .await
            .expect("FileSync sender reports protocol errors");
    }

    macro_rules! red_protocol_test {
        ($name:ident, $expectation:literal) => {
            #[tokio::test]
            #[ignore = "removed when the FileSync coordinator is implemented"]
            async fn $name() {
                let _ = red_baseline_stream().await.expect($expectation);
            }
        };
    }

    red_protocol_test!(
        diff_copy_red_baseline_streams_data_and_fin,
        "FileSync sender streams DATA and FIN"
    );
    red_protocol_test!(
        diff_copy_red_baseline_terminates_empty_files,
        "FileSync sender terminates empty files"
    );
    red_protocol_test!(
        diff_copy_red_baseline_interleaves_file_data,
        "FileSync sender interleaves file DATA"
    );
    red_protocol_test!(
        diff_copy_red_baseline_rejects_duplicate_requests,
        "FileSync sender rejects duplicate requests"
    );
    red_protocol_test!(
        diff_copy_red_baseline_rejects_unknown_requests,
        "FileSync sender rejects unknown requests"
    );
    red_protocol_test!(
        diff_copy_red_baseline_rejects_unexpected_packets,
        "FileSync sender rejects unexpected packets"
    );
    red_protocol_test!(
        diff_copy_red_baseline_rejects_early_fin,
        "FileSync sender rejects early FIN"
    );
    red_protocol_test!(
        diff_copy_red_baseline_rejects_input_eof,
        "FileSync sender rejects input EOF before FIN"
    );
    red_protocol_test!(
        diff_copy_red_baseline_handles_peer_errors,
        "FileSync sender handles peer errors"
    );
    red_protocol_test!(
        diff_copy_red_baseline_waits_for_accepted_jobs,
        "FileSync sender waits for accepted jobs"
    );
    red_protocol_test!(
        diff_copy_red_baseline_cancels_owned_tasks,
        "FileSync sender cancels owned tasks"
    );
    red_protocol_test!(
        diff_copy_red_baseline_reports_worker_errors,
        "FileSync sender reports worker errors"
    );
    red_protocol_test!(
        diff_copy_red_baseline_reports_worker_panics,
        "FileSync sender reports worker panics"
    );
    red_protocol_test!(
        diff_copy_red_baseline_applies_output_backpressure,
        "FileSync sender applies output backpressure"
    );
}
