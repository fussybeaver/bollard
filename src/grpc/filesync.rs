use std::{
    collections::{HashMap, HashSet, VecDeque},
    ffi::{OsStr, OsString},
    future::Future,
    panic::AssertUnwindSafe,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    thread::JoinHandle,
    time::Duration,
};

use bollard_buildkit_proto::{
    fsutil::types::{packet::PacketType, Packet, Stat},
    moby::filesync::v1::file_sync_server::FileSync,
};
use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
use futures_core::Stream;
use futures_util::{FutureExt, StreamExt};
use log::warn;
use tokio::io::AsyncReadExt;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;
use tonic::{metadata::MetadataMap, Request, Response, Status, Streaming};
#[cfg(unix)]
use xattr::FileExt;

use super::patternmatcher::{compile_component, PatternMatcher};

#[cfg(unix)]
use cap_std::fs::MetadataExt;
#[cfg(not(unix))]
use cap_std::time::SystemClock;

const DIR_NAME_METADATA: &str = "dir-name";
const INCLUDE_PATTERNS_METADATA: &str = "include-patterns";
const EXCLUDE_PATTERNS_METADATA: &str = "exclude-patterns";
const FOLLOW_PATHS_METADATA: &str = "followpaths";
const MAX_ENTRIES: usize = 100_000;
const MAX_PATH_LENGTH: usize = 4096;
const MAX_LINKNAME_LENGTH: usize = 4096;
const MAX_FILESYNC_PATTERNS: usize = 4096;
const MAX_FOLLOW_PATHS: usize = 1024;
const MAX_FOLLOW_RESOLVED_PATHS: usize = MAX_ENTRIES;
const MAX_FOLLOW_INSPECTED_ENTRIES: usize = MAX_ENTRIES;
const MAX_FOLLOW_DEPTH: usize = 256;
#[cfg(test)]
const ENTRY_QUEUE_CAPACITY: usize = 128;
const SCAN_BATCH_SIZE: usize = 128;
const FILE_JOB_QUEUE_CAPACITY: usize = 128;
const OUTPUT_QUEUE_CAPACITY: usize = 16;
const FILE_WORKER_COUNT: usize = 4;
const FILE_READ_BUFFER_SIZE: usize = 32 * 1024;
const FILESYNC_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

type JobSender = tokio::sync::mpsc::Sender<FileJob>;
type OutputReceiver = tokio::sync::mpsc::Receiver<Result<Packet, Status>>;
type ScanRequest = Pin<Box<dyn Future<Output = Result<ScanBatch, Status>> + Send>>;

struct FileSyncStart {
    session: FileSyncSession,
    jobs: JobSender,
    output: OutputReceiver,
}

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

#[derive(Clone, Debug)]
struct FileJob {
    id: u32,
    relative: PathBuf,
}

#[derive(Debug)]
struct FileTarget {
    regular: bool,
    relative: PathBuf,
}

/// Test-only fault-injection switches for exercising panic and race paths
/// through the real FileSync wire protocol. Always all-off in production:
/// `diff_copy` only parses them from request metadata under `cfg(test)`.
#[derive(Clone, Copy, Debug, Default)]
struct FaultInjection {
    panic_worker: bool,
    delay_scan: bool,
    panic_scanner: bool,
}

impl FaultInjection {
    #[cfg(test)]
    fn from_metadata(metadata: &tonic::metadata::MetadataMap) -> Self {
        Self {
            panic_worker: metadata.contains_key("x-test-panic-worker"),
            delay_scan: metadata.contains_key("x-test-delay-scan"),
            panic_scanner: metadata.contains_key("x-test-panic-scanner"),
        }
    }
}

struct FileSyncSession {
    cancellation: CancellationToken,
    scanner: Option<ScannerHandle>,
    workers: tokio::task::JoinSet<()>,
}

struct ScannerHandle {
    commands: tokio::sync::mpsc::Sender<ScannerCommand>,
    completion: tokio::sync::oneshot::Receiver<Result<(), Status>>,
    thread: Option<JoinHandle<()>>,
}

impl ScannerHandle {
    fn next_batch(&self, limit: usize) -> ScanRequest {
        let commands = self.commands.clone();
        Box::pin(async move {
            let (reply, response) = tokio::sync::oneshot::channel();
            commands
                .send(ScannerCommand::NextBatch { limit, reply })
                .await
                .map_err(|_| Status::internal("FileSync scanner stopped"))?;
            response
                .await
                .map_err(|_| Status::internal("FileSync scanner dropped its response"))?
        })
    }

    // Cancellation is also enforced by the token and dropping the command sender.
    fn cancel(&self) {
        let _ = self.commands.try_send(ScannerCommand::Cancel);
    }

    async fn join(mut self) -> Result<(), Status> {
        let commands = self.commands;
        drop(commands);
        self.completion
            .await
            .map_err(|_| Status::internal("FileSync scanner task failed"))??;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        Ok(())
    }
}

impl Drop for FileSyncSession {
    fn drop(&mut self) {
        self.cancellation.cancel();
        self.workers.abort_all();
    }
}

impl FileSyncSession {
    fn start(
        root: Arc<cap_std::fs::Dir>,
        selection: ScanSelection,
        faults: FaultInjection,
    ) -> Result<FileSyncStart, Status> {
        let cancellation = CancellationToken::new();
        let scanner_root = root.clone();
        let (commands_sender, mut commands_receiver) = mpsc::channel(1);
        let (jobs_sender, jobs_receiver) = mpsc::channel(FILE_JOB_QUEUE_CAPACITY);
        let (output_sender, output_receiver) = mpsc::channel(OUTPUT_QUEUE_CAPACITY);
        let scanner_cancellation = cancellation.clone();
        let (completion_sender, completion_receiver) = tokio::sync::oneshot::channel();
        let scanner_thread = std::thread::Builder::new()
            .name(String::from("bollard-filesync-scanner"))
            .spawn(move || {
                let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if faults.panic_scanner {
                        panic!("injected FileSync scanner panic");
                    }
                    let mut scanner = None::<Scanner>;
                    let mut scanner_root = Some(scanner_root);
                    let mut selection = Some(selection);
                    while let Some(command) = commands_receiver.blocking_recv() {
                        match command {
                            ScannerCommand::NextBatch { limit, reply } => {
                                let result = if let Some(scanner) = scanner.as_mut() {
                                    scanner.take_batch(limit, &scanner_cancellation, faults)
                                } else {
                                    match Scanner::new(
                                        scanner_root
                                            .take()
                                            .expect("scanner root is initialized once"),
                                        selection
                                            .take()
                                            .expect("scanner selection is initialized once"),
                                        &scanner_cancellation,
                                    ) {
                                        Ok(new_scanner) => {
                                            scanner = Some(new_scanner);
                                            scanner
                                                .as_mut()
                                                .expect("scanner was just initialized")
                                                .take_batch(limit, &scanner_cancellation, faults)
                                        }
                                        Err(error) => Err(error),
                                    }
                                };
                                let failed = result.is_err();
                                if reply.send(result).is_err() || failed {
                                    break;
                                }
                            }
                            ScannerCommand::Cancel => break,
                        }
                    }
                    Ok(())
                })) {
                    Ok(result) => result,
                    Err(_) => Err(Status::internal("FileSync scanner panicked")),
                };
                let _ = completion_sender.send(result);
            })
            .map_err(|error| Status::internal(format!("FileSync scanner task failed: {error}")))?;

        let jobs_receiver = Arc::new(Mutex::new(jobs_receiver));
        let mut workers = tokio::task::JoinSet::new();
        for _ in 0..FILE_WORKER_COUNT {
            let worker_jobs = Arc::clone(&jobs_receiver);
            let worker_output = output_sender.clone();
            let worker_root = root.clone();
            let worker_cancellation = cancellation.clone();
            workers.spawn(async move {
                let result = AssertUnwindSafe(async {
                    worker_loop(
                        worker_root,
                        worker_jobs,
                        worker_output.clone(),
                        worker_cancellation,
                        faults,
                    )
                    .await;
                })
                .catch_unwind()
                .await;
                if result.is_err() {
                    let _ = worker_output
                        .send(Err(Status::internal("FileSync worker panicked")))
                        .await;
                }
            });
        }
        drop(output_sender);

        Ok(FileSyncStart {
            session: Self {
                cancellation,
                scanner: Some(ScannerHandle {
                    commands: commands_sender,
                    completion: completion_receiver,
                    thread: Some(scanner_thread),
                }),
                workers,
            },
            jobs: jobs_sender,
            output: output_receiver,
        })
    }

    async fn shutdown(
        &mut self,
        jobs: &mut Option<tokio::sync::mpsc::Sender<FileJob>>,
        output: &mut tokio::sync::mpsc::Receiver<Result<Packet, Status>>,
    ) -> Result<(), Status> {
        self.cancellation.cancel();
        if let Some(scanner) = self.scanner.as_ref() {
            scanner.cancel();
        }
        output.close();
        jobs.take();

        let result = tokio::time::timeout(FILESYNC_SHUTDOWN_TIMEOUT, async {
            let scanner_result = if let Some(scanner) = self.scanner.take() {
                scanner.join().await
            } else {
                Ok(())
            };
            self.workers.abort_all();
            let mut worker_error = None;
            while let Some(result) = self.workers.join_next().await {
                if let Err(error) = result {
                    if !error.is_cancelled() && worker_error.is_none() {
                        worker_error = Some(Status::internal(format!(
                            "FileSync worker task failed: {error}"
                        )));
                    }
                }
            }
            scanner_result?;
            worker_error.map_or(Ok(()), Err)
        })
        .await;

        match result {
            Ok(result) => result,
            Err(_) => {
                self.workers.abort_all();
                Err(Status::deadline_exceeded(
                    "FileSync session cleanup exceeded its timeout",
                ))
            }
        }
    }
}

#[derive(Clone, Debug)]
enum ScanSelection {
    All,
    Filter {
        include: Option<PatternMatcher>,
        exclude: Option<PatternMatcher>,
    },
}

impl ScanSelection {
    fn from_patterns(include: &[String], exclude: &[String]) -> Result<Self, Status> {
        let include = PatternMatcher::new(include).map_err(|error| {
            Status::invalid_argument(format!("invalid FileSync include pattern: {error}"))
        })?;
        let exclude = PatternMatcher::new(exclude).map_err(|error| {
            Status::invalid_argument(format!("invalid FileSync exclude pattern: {error}"))
        })?;
        if include.is_none() && exclude.is_none() {
            Ok(Self::All)
        } else {
            Ok(Self::Filter { include, exclude })
        }
    }

    fn is_selected(&self, path: &Path) -> bool {
        let path = path
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/");
        match self {
            Self::All => true,
            Self::Filter { include, exclude } => {
                include
                    .as_ref()
                    .is_none_or(|matcher| matcher.matches_or_parent(&path))
                    && exclude
                        .as_ref()
                        .is_none_or(|matcher| !matcher.matches_or_parent(&path))
            }
        }
    }
}

struct ScanFrame {
    relative: PathBuf,
    directory: cap_std::fs::Dir,
    names: Vec<OsString>,
    next_name: usize,
    pending: bool,
}

struct PendingEntry {
    stat: Stat,
    regular: bool,
    relative: PathBuf,
}

struct ScanBudget {
    inspected: usize,
}

impl ScanBudget {
    fn inspect(&mut self) -> Result<(), Status> {
        if self.inspected >= MAX_ENTRIES {
            return Err(Status::resource_exhausted(
                "local source has too many entries",
            ));
        }
        self.inspected += 1;
        Ok(())
    }
}

struct FollowPathBudget {
    inspected: usize,
    resolved: usize,
}

impl FollowPathBudget {
    fn inspect(&mut self) -> Result<(), Status> {
        if self.inspected >= MAX_FOLLOW_INSPECTED_ENTRIES {
            return Err(Status::resource_exhausted(
                "FileSync followpaths inspected too many entries",
            ));
        }
        self.inspected += 1;
        Ok(())
    }

    fn resolve(&mut self, path: &Path) -> Result<String, Status> {
        if self.resolved >= MAX_FOLLOW_RESOLVED_PATHS {
            return Err(Status::resource_exhausted(
                "FileSync followpaths resolved too many paths",
            ));
        }
        self.resolved += 1;
        path_string(path)
    }
}

struct FollowPathContext {
    visited: HashSet<String>,
    resolved: Vec<String>,
    budget: FollowPathBudget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransferPhase {
    Enumerating,
    AwaitingFin,
    Finishing,
}

enum ScannerCommand {
    NextBatch {
        limit: usize,
        reply: tokio::sync::oneshot::Sender<Result<ScanBatch, Status>>,
    },
    Cancel,
}

struct ScanBatch {
    entries: Vec<SourceEntry>,
    finished: bool,
}

enum SessionEvent {
    Scan(Result<ScanBatch, Status>),
    Stat,
    Packet(Option<Result<Packet, Status>>),
    Output(Option<Result<Packet, Status>>),
    Job(Result<FileJob, FileJob>),
}

async fn next_session_event(
    scan_request: Option<&mut ScanRequest>,
    input: &mut Pin<Box<Streaming<Packet>>>,
    output: &mut tokio::sync::mpsc::Receiver<Result<Packet, Status>>,
    jobs: &JobSender,
    queued_job: Option<&FileJob>,
    pending_stat: bool,
) -> SessionEvent {
    let mut pending_scan: ScanRequest = Box::pin(std::future::pending());
    let scan_request = scan_request.unwrap_or(&mut pending_scan);

    if let Some(job) = queued_job {
        let job = job.clone();
        let sent_job = job.clone();
        tokio::select! {
            biased;
            result = jobs.send(job) => {
                match result {
                    Ok(()) => SessionEvent::Job(Ok(sent_job)),
                    Err(error) => SessionEvent::Job(Err(error.0)),
                }
            }
            _stat = std::future::ready(()), if pending_stat => SessionEvent::Stat,
            batch = scan_request => SessionEvent::Scan(batch),
            packet = output.recv() => SessionEvent::Output(packet),
        }
    } else {
        tokio::select! {
            biased;
            packet = input.next() => SessionEvent::Packet(packet),
            _stat = std::future::ready(()), if pending_stat => SessionEvent::Stat,
            batch = scan_request => SessionEvent::Scan(batch),
            packet = output.recv() => SessionEvent::Output(packet),
        }
    }
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
        let options = parse_options(request.metadata())?;
        validate_options(request.metadata())?;
        let name = options
            .dir_name
            .clone()
            .ok_or_else(|| Status::not_found("local source name is missing"))?;
        let root = lookup_mount(&self.mounts, &name)?;
        let selection = tokio::task::spawn_blocking({
            let root = root.clone();
            move || scan_selection(&root, &options)
        })
        .await
        .map_err(|error| {
            Status::internal(format!("FileSync selection worker failed: {error}"))
        })??;
        #[cfg(test)]
        let faults = FaultInjection::from_metadata(request.metadata());
        #[cfg(not(test))]
        let faults = FaultInjection::default();
        let FileSyncStart {
            mut session,
            jobs: jobs_sender,
            output: mut output_receiver,
        } = FileSyncSession::start(root, selection, faults)?;
        let mut jobs_sender = Some(jobs_sender);
        let mut input = Box::pin(request.into_inner());

        let output = async_stream::stream! {
            let mut positions = HashMap::<u32, FileTarget>::new();
            let mut pending_jobs = HashMap::<u32, ()>::new();
            let mut queued_job = None::<FileJob>;
            let mut phase = TransferPhase::Enumerating;
            let mut scan_request = None::<ScanRequest>;
            let mut pending_entries = VecDeque::<SourceEntry>::new();
            // Finalize STAT only after the final batch has been emitted.
            let mut scan_completion_pending = false;

            macro_rules! fail {
                ($error:expr) => {{
                    let error = $error;
                    yield Ok(error_packet(&error));
                    let _ = scan_request.take();
                    if let Err(cleanup_error) = session
                        .shutdown(&mut jobs_sender, &mut output_receiver)
                        .await
                    {
                        warn!("FileSync cleanup failed after protocol error: {cleanup_error}");
                    }
                    yield Err(error);
                    break;
                }};
            }

            loop {
                let need_batch = if scan_completion_pending {
                    pending_entries.is_empty()
                } else {
                    pending_entries.len() < SCAN_BATCH_SIZE
                };
                if phase == TransferPhase::Enumerating
                    && scan_request.is_none()
                    && need_batch
                {
                    let scanner = session
                        .scanner
                        .as_ref()
                        .expect("scanner remains available while enumerating");
                    scan_request = Some(if scan_completion_pending {
                        Box::pin(std::future::ready(Ok(ScanBatch {
                            entries: Vec::new(),
                            finished: true,
                        })))
                    } else {
                        scanner.next_batch(SCAN_BATCH_SIZE)
                    });
                }
                let event = next_session_event(
                    scan_request.as_mut(),
                    &mut input,
                    &mut output_receiver,
                    jobs_sender
                        .as_ref()
                        .expect("FileSync jobs remain available during transfer"),
                    queued_job.as_ref(),
                    !pending_entries.is_empty(),
                ).await;
                match event {
                    SessionEvent::Job(job) => match job {
                        Ok(job) => {
                            queued_job = None;
                            pending_jobs.insert(job.id, ());
                        }
                        Err(job) => fail!(Status::internal(format!(
                            "FileSync file-job queue closed for request {}",
                            job.id
                        ))),
                    },
                    SessionEvent::Stat => {
                        let entry = pending_entries
                            .pop_front()
                            .expect("STAT event has a pending entry");
                        let SourceEntry {
                            stat,
                            position,
                            regular,
                            relative,
                        } = entry;
                        positions.insert(position, FileTarget { regular, relative });
                        yield Ok(stat_entry_packet(stat));
                    }
                    SessionEvent::Scan(result) => {
                        scan_request = None;
                        match result {
                        Ok(ScanBatch { entries, finished }) => {
                            pending_entries.extend(entries);
                            if finished {
                                if scan_completion_pending {
                                    scan_completion_pending = false;
                                    if let Some(scanner) = session.scanner.take() {
                                        if let Err(join_error) = scanner.join().await {
                                            fail!(Status::internal(format!(
                                                "FileSync scanner task failed: {join_error}"
                                            )));
                                        }
                                    }
                                    yield Ok(stat_terminator());
                                    phase = TransferPhase::AwaitingFin;
                                } else {
                                    scan_completion_pending = true;
                                }
                            }
                        }
                        Err(error) => fail!(error),
                        }
                    }
                    SessionEvent::Packet(packet) => {
                        let packet = match packet {
                            Some(Ok(packet)) => packet,
                            Some(Err(error)) => fail!(error),
                            None => fail!(Status::failed_precondition(
                                "FileSync stream ended before PACKET_FIN",
                            )),
                        };
                        let packet_type = match PacketType::try_from(packet.r#type) {
                            Ok(packet_type) => packet_type,
                            Err(_) => fail!(Status::invalid_argument(
                                "unknown FileSync packet type",
                            )),
                        };
                        match packet_type {
                            PacketType::PacketErr => {
                                fail!(Status::aborted(
                                    "BuildKit aborted the FileSync transfer",
                                ));
                            }
                            PacketType::PacketReq => {
                                if phase == TransferPhase::Finishing {
                                    fail!(Status::failed_precondition(
                                        "FileSync received PACKET_REQ after PACKET_FIN",
                                    ));
                                }
                                let target = positions.remove(&packet.id).ok_or_else(|| {
                                    Status::invalid_argument("invalid or repeated FileSync request ID")
                                });
                                let target = match target {
                                    Ok(target) if target.regular => target,
                                    Ok(_) => {
                                        fail!(Status::invalid_argument(
                                            "FileSync request does not identify a regular file",
                                        ));
                                    }
                                    Err(error) => {
                                        fail!(error);
                                    }
                                };
                                let job = FileJob {
                                    id: packet.id,
                                    relative: target.relative,
                                };
                                if jobs_sender.is_some() {
                                    debug_assert!(
                                        queued_job.is_none(),
                                        "queued FileJob would be overwritten"
                                    );
                                    queued_job = Some(job);
                                } else {
                                    fail!(Status::failed_precondition(
                                        "FileSync session is shutting down",
                                    ));
                                }
                            }
                            PacketType::PacketFin => {
                                if phase == TransferPhase::Enumerating {
                                    fail!(Status::failed_precondition(
                                        "FileSync received PACKET_FIN before STAT termination",
                                    ));
                                }
                                if phase == TransferPhase::Finishing {
                                    fail!(Status::failed_precondition(
                                        "FileSync received repeated PACKET_FIN",
                                    ));
                                }
                                phase = TransferPhase::Finishing;
                                if pending_jobs.is_empty() {
                                    yield Ok(fin_response_packet());
                                    match session
                                        .shutdown(&mut jobs_sender, &mut output_receiver)
                                        .await
                                    {
                                        Ok(()) => break,
                                        Err(error) => fail!(error),
                                    }
                                }
                            }
                            PacketType::PacketStat | PacketType::PacketData => {
                                fail!(Status::invalid_argument(
                                    "unexpected packet type from FileSync receiver",
                                ));
                            }
                        }
                    }
                    SessionEvent::Output(output) => match output {
                        Some(Ok(packet)) => {
                            let eof = packet.r#type == PacketType::PacketData as i32
                                && packet.data.is_empty();
                            if eof && pending_jobs.remove(&packet.id).is_none() {
                                fail!(Status::internal(
                                    "FileSync worker emitted an unexpected EOF",
                                ));
                            }
                            yield Ok(packet);
                            if phase == TransferPhase::Finishing && pending_jobs.is_empty() {
                                yield Ok(fin_response_packet());
                                match session
                                    .shutdown(&mut jobs_sender, &mut output_receiver)
                                    .await
                                {
                                    Ok(()) => break,
                                    Err(error) => fail!(error),
                                }
                            }
                        }
                        Some(Err(error)) => {
                            fail!(error)
                        }
                        None => {
                            fail!(Status::internal("FileSync output channel closed"))
                        }
                    },
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

async fn worker_loop(
    root: Arc<cap_std::fs::Dir>,
    jobs: Arc<Mutex<tokio::sync::mpsc::Receiver<FileJob>>>,
    output: tokio::sync::mpsc::Sender<Result<Packet, Status>>,
    cancellation: CancellationToken,
    faults: FaultInjection,
) {
    let mut buffer = vec![0_u8; FILE_READ_BUFFER_SIZE];
    loop {
        let job = {
            let mut jobs = jobs.lock().await;
            jobs.recv().await
        };
        let Some(job) = job else { return };
        if cancellation.is_cancelled() {
            return;
        }
        if faults.panic_worker {
            panic!("injected FileSync worker panic");
        }

        let file = match open_regular_file(root.clone(), job.relative.clone()).await {
            Ok(file) => file,
            Err(error) => {
                let _ = output.send(Err(error)).await;
                return;
            }
        };
        let mut file = file;
        loop {
            if cancellation.is_cancelled() {
                return;
            }
            let count = match file.read(&mut buffer).await {
                Ok(count) => count,
                Err(error) => {
                    let _ = output
                        .send(Err(Status::internal(format!(
                            "failed to read local source file {:?}: {error}",
                            job.relative
                        ))))
                        .await;
                    return;
                }
            };
            if count == 0 {
                if output
                    .send(Ok(file_data_packet(job.id, Vec::new())))
                    .await
                    .is_err()
                {
                    return;
                }
                break;
            }
            if output
                .send(Ok(file_data_packet(job.id, buffer[..count].to_vec())))
                .await
                .is_err()
            {
                return;
            }
        }
    }
}

async fn open_regular_file(
    root: Arc<cap_std::fs::Dir>,
    relative: PathBuf,
) -> Result<tokio::fs::File, Status> {
    let file = tokio::task::spawn_blocking(move || {
        let mut options = cap_std::fs::OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        let file = root
            .open_with(&relative, &options)
            .map_err(|error| filesystem_error("open", &relative, error))?;
        let metadata = file
            .metadata()
            .map_err(|error| filesystem_error("stat", &relative, error))?;
        if !metadata.file_type().is_file() {
            return Err(Status::invalid_argument(format!(
                "FileSync request is no longer a regular file: {relative:?}"
            )));
        }
        Ok(file.into_std())
    })
    .await
    .map_err(|error| Status::internal(format!("FileSync file worker failed: {error}")))??;
    Ok(tokio::fs::File::from_std(file))
}

#[derive(Clone, Debug, Default)]
struct FileSyncOptions {
    dir_name: Option<String>,
    include_patterns: Vec<String>,
    exclude_patterns: Vec<String>,
    follow_paths: Vec<String>,
}

fn parse_options(metadata: &MetadataMap) -> Result<FileSyncOptions, Status> {
    Ok(FileSyncOptions {
        dir_name: metadata_values(metadata, DIR_NAME_METADATA)?
            .into_iter()
            .next(),
        include_patterns: metadata_values(metadata, INCLUDE_PATTERNS_METADATA)?,
        exclude_patterns: metadata_values(metadata, EXCLUDE_PATTERNS_METADATA)?,
        follow_paths: metadata_values(metadata, FOLLOW_PATHS_METADATA)?,
    })
}

fn metadata_values(metadata: &MetadataMap, key: &str) -> Result<Vec<String>, Status> {
    let encoded = metadata
        .get(format!("{key}-encoded"))
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| matches!(value, "1" | "true" | "TRUE" | "True" | "t" | "T"));
    metadata
        .get_all(key)
        .iter()
        .map(|value| {
            let value = value
                .to_str()
                .map_err(|_| Status::invalid_argument(format!("invalid {key} metadata")))?;
            if encoded {
                let encoded_value = format!("value={value}");
                Ok(url::form_urlencoded::parse(encoded_value.as_bytes())
                    .next()
                    .map(|(_, value)| value.into_owned())
                    .unwrap_or_default())
            } else {
                Ok(value.to_owned())
            }
        })
        .collect()
}

fn scan_selection(
    root: &cap_std::fs::Dir,
    options: &FileSyncOptions,
) -> Result<ScanSelection, Status> {
    if options.include_patterns.len() > MAX_FILESYNC_PATTERNS
        || options.exclude_patterns.len() > MAX_FILESYNC_PATTERNS
    {
        return Err(Status::resource_exhausted(
            "FileSync request contains too many filter patterns",
        ));
    }
    let mut include_patterns = options.include_patterns.clone();
    let follow_patterns = resolve_follow_paths(root, &options.follow_paths)?;
    if include_patterns.len().saturating_add(follow_patterns.len()) > MAX_FILESYNC_PATTERNS {
        return Err(Status::resource_exhausted(
            "FileSync request resolves to too many filter patterns",
        ));
    }
    if follow_patterns.iter().any(|path| path == ".") {
        return ScanSelection::from_patterns(&include_patterns, &options.exclude_patterns);
    }
    include_patterns.extend(follow_patterns);
    ScanSelection::from_patterns(&include_patterns, &options.exclude_patterns)
}

fn resolve_follow_paths(root: &cap_std::fs::Dir, paths: &[String]) -> Result<Vec<String>, Status> {
    if paths.len() > MAX_FOLLOW_PATHS {
        return Err(Status::resource_exhausted(
            "FileSync request contains too many followpaths",
        ));
    }
    let mut context = FollowPathContext {
        visited: HashSet::new(),
        resolved: Vec::new(),
        budget: FollowPathBudget {
            inspected: 0,
            resolved: 0,
        },
    };
    for path in paths {
        if path == "." {
            return Ok(vec![String::from(".")]);
        }
        context
            .resolved
            .push(context.budget.resolve(Path::new(path))?);
        let components = path.split('/').map(str::to_owned).collect::<Vec<_>>();
        if components.len() > MAX_FOLLOW_DEPTH {
            return Err(Status::resource_exhausted(
                "FileSync followpaths path is too deep",
            ));
        }
        resolve_follow_components(root, Path::new(""), &components, &mut context, 0)?;
    }
    context.resolved.sort_unstable();
    context.resolved.dedup();
    let mut deduped = Vec::with_capacity(context.resolved.len());
    for path in context.resolved {
        if deduped.last().is_some_and(|parent: &String| {
            path.strip_prefix(parent)
                .is_some_and(|suffix| suffix.starts_with('/'))
        }) {
            continue;
        }
        deduped.push(path);
    }
    Ok(deduped)
}

fn resolve_follow_components(
    root: &cap_std::fs::Dir,
    current: &Path,
    components: &[String],
    context: &mut FollowPathContext,
    depth: usize,
) -> Result<(), Status> {
    if depth > MAX_FOLLOW_DEPTH {
        return Err(Status::resource_exhausted(
            "FileSync followpaths resolution is too deep",
        ));
    }
    let Some(component) = components.first() else {
        if !current.as_os_str().is_empty() {
            context.resolved.push(context.budget.resolve(current)?);
        }
        return Ok(());
    };

    if contains_wildcard(component) {
        let component_pattern = compile_component(component).ok_or_else(|| {
            Status::invalid_argument(format!(
                "invalid FileSync followpath pattern: {component:?}"
            ))
        })?;
        let directory = if current.as_os_str().is_empty() {
            root.try_clone()
        } else {
            root.open_dir(current)
        };
        let directory = match directory {
            Ok(directory) => directory,
            Err(error) if is_followpath_not_found(&error) => return Ok(()),
            Err(error) => {
                return Err(filesystem_error(
                    "open followpaths directory",
                    current,
                    error,
                ))
            }
        };
        let mut names = Vec::new();
        for entry in directory
            .entries()
            .map_err(|error| filesystem_error("read followpaths directory", current, error))?
        {
            context.budget.inspect()?;
            let entry = entry
                .map_err(|error| filesystem_error("read followpaths entry", current, error))?;
            names.push(entry.file_name());
        }
        names.sort_unstable();
        for name in names {
            let Some(name) = name.to_str() else {
                log::debug!("skipping non-UTF-8 followpath entry: {name:?}");
                continue;
            };
            if component_pattern.matches(name) {
                resolve_follow_entry(root, current, name, &components[1..], context, depth + 1)?;
            }
        }
        return Ok(());
    }

    context.budget.inspect()?;
    resolve_follow_entry(
        root,
        current,
        component,
        &components[1..],
        context,
        depth + 1,
    )
}

fn resolve_follow_entry(
    root: &cap_std::fs::Dir,
    current: &Path,
    name: &str,
    remaining: &[String],
    context: &mut FollowPathContext,
    depth: usize,
) -> Result<(), Status> {
    let next = current.join(name);
    let metadata = match root.symlink_metadata(&next) {
        Ok(metadata) => metadata,
        Err(error) if is_followpath_not_found(&error) => return Ok(()),
        Err(error) => return Err(filesystem_error("stat followpaths entry", &next, error)),
    };
    let next_string = path_string(&next)?;
    if metadata.file_type().is_symlink() {
        if !context.visited.insert(next_string.clone()) {
            return Ok(());
        }
        context.resolved.push(context.budget.resolve(&next)?);
        let parent = next.parent().unwrap_or_else(|| Path::new(""));
        let link = root
            .read_link_contents(&next)
            .map_err(|error| filesystem_error("read followpaths symlink", &next, error))?;
        let Some(target) = normalize_follow_target(parent, &link) else {
            return Ok(());
        };
        if target.as_os_str().is_empty() {
            context.resolved.push(String::from("."));
            return Ok(());
        }
        let mut target_components = target
            .components()
            .filter_map(|component| match component {
                std::path::Component::Normal(value) => value.to_str().map(str::to_owned),
                _ => None,
            })
            .collect::<Vec<_>>();
        target_components.extend_from_slice(remaining);
        return resolve_follow_components(
            root,
            Path::new(""),
            &target_components,
            context,
            depth + 1,
        );
    }
    if remaining.is_empty() {
        context.resolved.push(context.budget.resolve(&next)?);
    } else if !metadata.file_type().is_dir() {
        return Ok(());
    } else {
        resolve_follow_components(root, &next, remaining, context, depth)?;
    }
    Ok(())
}

fn normalize_follow_target(parent: &Path, link: &Path) -> Option<PathBuf> {
    let mut result = if link.is_absolute() {
        PathBuf::new()
    } else {
        parent.to_path_buf()
    };
    for component in link.components() {
        match component {
            std::path::Component::RootDir | std::path::Component::CurDir => {}
            // Match fsutil's filepath.Join(root, target) behavior: a link
            // that climbs above the mount root remains clamped at the root.
            std::path::Component::ParentDir => {
                result.pop();
            }
            std::path::Component::Normal(value) => result.push(value),
            #[cfg(windows)]
            std::path::Component::Prefix(prefix) => match prefix.kind() {
                std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_) => {
                    // fsutil strips a Windows drive prefix before resolving
                    // an absolute link target relative to the mount root.
                    result = PathBuf::new();
                }
                _ => return None,
            },
            #[cfg(not(windows))]
            std::path::Component::Prefix(_) => return None,
        }
    }
    Some(result)
}

fn is_followpath_not_found(error: &std::io::Error) -> bool {
    if error.kind() == std::io::ErrorKind::NotFound {
        return true;
    }
    #[cfg(windows)]
    {
        return error.raw_os_error() == Some(123);
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn contains_wildcard(value: &str) -> bool {
    let mut escaped = false;
    for character in value.chars() {
        if cfg!(not(windows)) && escaped {
            escaped = false;
            continue;
        }
        if cfg!(not(windows)) && character == '\\' {
            escaped = true;
            continue;
        }
        if matches!(character, '*' | '?' | '[') {
            return true;
        }
    }
    false
}

fn path_string(path: &Path) -> Result<String, Status> {
    let value = path
        .to_str()
        .ok_or_else(|| {
            Status::invalid_argument("local source contains a non-UTF-8 followpaths path")
        })?
        .replace(std::path::MAIN_SEPARATOR, "/");
    if value.len() > MAX_PATH_LENGTH {
        return Err(Status::invalid_argument(
            "invalid FileSync followpaths path",
        ));
    }
    Ok(value)
}

fn mount_name(metadata: &MetadataMap) -> Result<String, Status> {
    parse_options(metadata)?
        .dir_name
        .ok_or_else(|| Status::not_found("local source name is missing"))
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
    let options = parse_options(metadata)?;
    for value in options.follow_paths {
        if value == "." {
            continue;
        }
        let path = PathBuf::from(&value);
        if value.is_empty()
            || path.is_absolute()
            || path.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir | std::path::Component::RootDir
                )
            })
            || path.as_os_str().len() > MAX_PATH_LENGTH
        {
            return Err(Status::invalid_argument(
                "invalid FileSync followpaths path",
            ));
        }
        PatternMatcher::new(std::slice::from_ref(&value)).map_err(|error| {
            Status::invalid_argument(format!("invalid FileSync followpath pattern: {error}"))
        })?;
    }
    PatternMatcher::new(&options.include_patterns).map_err(|error| {
        Status::invalid_argument(format!("invalid FileSync include pattern: {error}"))
    })?;
    PatternMatcher::new(&options.exclude_patterns).map_err(|error| {
        Status::invalid_argument(format!("invalid FileSync exclude pattern: {error}"))
    })?;
    Ok(())
}

#[cfg(test)]
fn scan_entries(
    root: Arc<cap_std::fs::Dir>,
    sender: tokio::sync::mpsc::Sender<Result<SourceEntry, Status>>,
) -> Result<(), Status> {
    let cancellation = CancellationToken::new();
    scan_entries_with_selection(
        root,
        sender,
        ScanSelection::All,
        cancellation,
        FaultInjection::default(),
    )
}

#[cfg(test)]
fn scan_entries_with_selection(
    root: Arc<cap_std::fs::Dir>,
    sender: tokio::sync::mpsc::Sender<Result<SourceEntry, Status>>,
    selection: ScanSelection,
    cancellation: CancellationToken,
    faults: FaultInjection,
) -> Result<(), Status> {
    let mut scanner = Scanner::new(root, selection, &cancellation)?;
    loop {
        let batch = scanner.take_batch(SCAN_BATCH_SIZE, &cancellation, faults)?;
        let finished = batch.finished;
        for entry in batch.entries {
            if sender.blocking_send(Ok(entry)).is_err() {
                return Ok(());
            }
        }
        if finished {
            return Ok(());
        }
    }
}

struct Scanner {
    selection: ScanSelection,
    budget: ScanBudget,
    frames: Vec<ScanFrame>,
    position: u32,
    pending: Vec<PendingEntry>,
    ready: VecDeque<SourceEntry>,
    seen_hardlinks: HashMap<(u64, u64), String>,
}

impl Scanner {
    fn new(
        root: Arc<cap_std::fs::Dir>,
        selection: ScanSelection,
        cancellation: &CancellationToken,
    ) -> Result<Self, Status> {
        let root = root.try_clone().map_err(|error| {
            Status::internal(format!("failed to retain local source root: {error}"))
        })?;
        let mut budget = ScanBudget { inspected: 0 };
        let names = sorted_names(&root, &mut budget, cancellation)?;
        Ok(Self {
            selection,
            budget,
            frames: vec![ScanFrame {
                relative: PathBuf::new(),
                directory: root,
                names,
                next_name: 0,
                pending: false,
            }],
            position: 0,
            pending: Vec::new(),
            ready: VecDeque::new(),
            seen_hardlinks: HashMap::new(),
        })
    }

    fn take_batch(
        &mut self,
        limit: usize,
        cancellation: &CancellationToken,
        faults: FaultInjection,
    ) -> Result<ScanBatch, Status> {
        if limit == 0 {
            return Err(Status::invalid_argument(
                "FileSync scanner batch limit must be positive",
            ));
        }

        let mut entries = Vec::with_capacity(limit);
        loop {
            while entries.len() < limit {
                if let Some(entry) = self.ready.pop_front() {
                    entries.push(entry);
                } else {
                    break;
                }
            }
            if entries.len() == limit {
                return Ok(ScanBatch {
                    entries,
                    finished: false,
                });
            }
            if cancellation.is_cancelled() {
                return Ok(ScanBatch {
                    entries,
                    finished: true,
                });
            }

            let Some(mut frame) = self.frames.pop() else {
                return Ok(ScanBatch {
                    entries,
                    finished: true,
                });
            };
            if faults.delay_scan {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            let Some(name) = frame.names.get(frame.next_name).cloned() else {
                if frame.pending {
                    self.pending.pop();
                }
                continue;
            };
            frame.next_name += 1;

            let relative = frame.relative.join(&name);
            let metadata = frame
                .directory
                .symlink_metadata(&name)
                .map_err(|error| filesystem_error("stat", &relative, error))?;
            if !self.selection.is_selected(&relative) {
                if metadata.file_type().is_dir() {
                    let (stat, regular) = source_stat(
                        &frame.directory,
                        &name,
                        &relative,
                        &metadata,
                        &mut self.seen_hardlinks,
                    )?;
                    let directory = frame
                        .directory
                        .open_dir(&name)
                        .map_err(|error| filesystem_error("open directory", &relative, error))?;
                    self.pending.push(PendingEntry {
                        stat,
                        regular,
                        relative: relative.clone(),
                    });
                    self.frames.push(frame);
                    self.frames.push(ScanFrame {
                        relative,
                        names: sorted_names(&directory, &mut self.budget, cancellation)?,
                        directory,
                        next_name: 0,
                        pending: true,
                    });
                } else {
                    self.frames.push(frame);
                }
                continue;
            }

            let (stat, regular) = source_stat(
                &frame.directory,
                &name,
                &relative,
                &metadata,
                &mut self.seen_hardlinks,
            )?;
            for pending_entry in std::mem::take(&mut self.pending) {
                let entry = self.source_entry(
                    pending_entry.stat,
                    pending_entry.regular,
                    pending_entry.relative,
                )?;
                self.ready.push_back(entry);
            }
            let entry = self.source_entry(stat, regular, relative.clone())?;
            self.ready.push_back(entry);

            let file_type = metadata.file_type();
            let child = if file_type.is_dir() {
                let directory = frame
                    .directory
                    .open_dir(&name)
                    .map_err(|error| filesystem_error("open directory", &relative, error))?;
                Some(ScanFrame {
                    relative,
                    names: sorted_names(&directory, &mut self.budget, cancellation)?,
                    directory,
                    next_name: 0,
                    pending: false,
                })
            } else {
                None
            };

            self.frames.push(frame);
            if let Some(child) = child {
                self.frames.push(child);
            }
        }
    }

    fn source_entry(
        &mut self,
        stat: Stat,
        regular: bool,
        relative: PathBuf,
    ) -> Result<SourceEntry, Status> {
        if self.position as usize >= MAX_ENTRIES {
            return Err(Status::resource_exhausted(
                "local source has too many entries",
            ));
        }
        let position = self.position;
        self.position = self
            .position
            .checked_add(1)
            .ok_or_else(|| Status::resource_exhausted("local source entry ID exhausted"))?;
        Ok(SourceEntry {
            stat,
            position,
            regular,
            relative,
        })
    }
}

fn sorted_names(
    directory: &cap_std::fs::Dir,
    budget: &mut ScanBudget,
    cancellation: &CancellationToken,
) -> Result<Vec<OsString>, Status> {
    let mut names = Vec::new();
    for entry in directory.entries().map_err(|error| {
        Status::internal(format!("failed to read local source directory: {error}"))
    })? {
        if cancellation.is_cancelled() {
            return Ok(names);
        }
        let entry = entry.map_err(|error| {
            Status::internal(format!("failed to read local source entry: {error}"))
        })?;
        budget.inspect()?;
        names.push(entry.file_name());
    }
    names.sort_unstable();
    Ok(names)
}

fn source_stat(
    directory: &cap_std::fs::Dir,
    name: &OsStr,
    relative: &Path,
    metadata: &cap_std::fs::Metadata,
    _seen_hardlinks: &mut HashMap<(u64, u64), String>,
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

    #[cfg(unix)]
    let (linkname, size) = {
        let mut linkname = linkname;
        if regular && metadata.nlink() > 1 {
            let identity = (metadata.dev(), metadata.ino());
            if let Some(first) = _seen_hardlinks.get(&identity) {
                linkname = first.clone();
            } else {
                _seen_hardlinks.insert(identity, path.to_owned());
            }
        }
        (linkname, size)
    };

    // Symlink xattrs require path-based no-follow calls. The mount retains only
    // a capability directory, so deriving an ambient path would weaken that
    // boundary; cap-std's no-follow file handle cannot read symlink xattrs.
    // Keep this divergence from fsutil explicit until a capability-relative
    // llistxattr/lgetxattr implementation is available.
    #[cfg(unix)]
    let xattrs = if file_type.is_dir() || regular {
        entry_xattrs(directory, name, file_type.is_dir())?
    } else {
        HashMap::new()
    };

    #[cfg(not(unix))]
    let xattrs = HashMap::new();

    Ok((
        Stat {
            path: path.replace(std::path::MAIN_SEPARATOR, "/"),
            mode,
            uid: file_uid(metadata),
            gid: file_gid(metadata),
            size,
            mod_time: modification_time(metadata),
            linkname,
            xattrs,
            ..Default::default()
        },
        regular,
    ))
}

#[cfg(unix)]
fn entry_xattrs(
    directory: &cap_std::fs::Dir,
    name: &OsStr,
    _is_directory: bool,
) -> Result<HashMap<String, Vec<u8>>, Status> {
    let mut options = cap_std::fs::OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let file = directory
        .open_with(name, &options)
        .map_err(|error| filesystem_error("open xattr entry", Path::new(name), error))?
        .into_std();
    let names = match file.list_xattr() {
        Ok(names) => names,
        Err(error) if error.kind() == std::io::ErrorKind::Unsupported => return Ok(HashMap::new()),
        Err(error) => return Err(filesystem_error("list xattrs", Path::new(name), error)),
    };
    let mut xattrs = HashMap::new();
    for name in names {
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if !super::fsutil::is_transferable_xattr(name_str) {
            continue;
        }
        if let Some(value) = file
            .get_xattr(&name)
            .map_err(|error| filesystem_error("read xattr", Path::new(&name), error))?
        {
            xattrs.insert(name_str.to_owned(), value);
        }
    }
    Ok(xattrs)
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
        .and_then(|time| time.duration_since(SystemClock::UNIX_EPOCH).ok())
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

fn file_data_packet(id: u32, data: Vec<u8>) -> Packet {
    Packet {
        r#type: PacketType::PacketData as i32,
        id,
        data,
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
use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
#[cfg(test)]
use tonic::transport::Server;

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
pub(crate) fn request_packet(id: u32) -> Packet {
    Packet {
        r#type: PacketType::PacketReq as i32,
        id,
        ..Default::default()
    }
}

#[cfg(test)]
pub(crate) fn data_packet(id: u32, data: impl Into<Vec<u8>>) -> Packet {
    file_data_packet(id, data.into())
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
async fn start_filesync_server(
    root: Arc<cap_std::fs::Dir>,
) -> (
    std::net::SocketAddr,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    start_filesync_server_with_mounts(HashMap::from([(String::from("context"), root)])).await
}

#[cfg(test)]
async fn start_filesync_server_with_mounts(
    mounts: HashMap<String, Arc<cap_std::fs::Dir>>,
) -> (
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
async fn open_filesync_transfer(
    root: Arc<cap_std::fs::Dir>,
) -> (
    mpsc::Sender<Packet>,
    Streaming<Packet>,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    open_filesync_transfer_with_metadata(root, "context", FaultInjection::default()).await
}

#[cfg(test)]
async fn open_filesync_transfer_with_metadata(
    root: Arc<cap_std::fs::Dir>,
    dir_name: &str,
    faults: FaultInjection,
) -> (
    mpsc::Sender<Packet>,
    Streaming<Packet>,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    open_filesync_transfer_from_mounts(
        HashMap::from([(String::from("context"), root)]),
        dir_name,
        faults,
    )
    .await
}

#[cfg(test)]
async fn open_filesync_transfer_from_mounts(
    mounts: HashMap<String, Arc<cap_std::fs::Dir>>,
    dir_name: &str,
    faults: FaultInjection,
) -> (
    mpsc::Sender<Packet>,
    Streaming<Packet>,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    let (address, shutdown_sender, server_task) = start_filesync_server_with_mounts(mounts).await;
    let mut client =
        bollard_buildkit_proto::moby::filesync::v1::file_sync_client::FileSyncClient::connect(
            format!("http://{address}"),
        )
        .await
        .expect("FileSync client connects");
    let (sender, receiver) = mpsc::channel(OUTPUT_QUEUE_CAPACITY);
    let mut request = Request::new(ReceiverStream::new(receiver));
    request.metadata_mut().insert(
        DIR_NAME_METADATA,
        tonic::metadata::MetadataValue::try_from(dir_name).expect("metadata value is valid"),
    );
    if faults.panic_worker {
        request.metadata_mut().insert(
            "x-test-panic-worker",
            tonic::metadata::MetadataValue::try_from("1").expect("metadata value is valid"),
        );
    }
    if faults.delay_scan {
        request.metadata_mut().insert(
            "x-test-delay-scan",
            tonic::metadata::MetadataValue::try_from("1").expect("metadata value is valid"),
        );
    }
    if faults.panic_scanner {
        request.metadata_mut().insert(
            "x-test-panic-scanner",
            tonic::metadata::MetadataValue::try_from("1").expect("metadata value is valid"),
        );
    }
    let responses = client
        .diff_copy(request)
        .await
        .expect("DiffCopy starts")
        .into_inner();
    (sender, responses, shutdown_sender, server_task)
}

#[cfg(test)]
async fn read_stat_terminator(responses: &mut Streaming<Packet>) -> Vec<Packet> {
    let mut packets = Vec::new();
    loop {
        let packet = responses
            .message()
            .await
            .expect("STAT response succeeds")
            .expect("STAT response exists");
        assert_eq!(packet.r#type, PacketType::PacketStat as i32);
        let done = packet.stat.is_none();
        packets.push(packet);
        if done {
            return packets;
        }
    }
}

#[cfg(test)]
async fn expect_protocol_error(responses: &mut Streaming<Packet>) -> tonic::Status {
    loop {
        let packet = responses
            .message()
            .await
            .expect("FileSync error packet succeeds")
            .expect("FileSync error packet exists");
        if packet.r#type == PacketType::PacketErr as i32 {
            return responses
                .message()
                .await
                .expect_err("FileSync stream returns its protocol error");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn scan_fixture_in_batches(root: Arc<cap_std::fs::Dir>, limit: usize) -> Vec<SourceEntry> {
        let cancellation = CancellationToken::new();
        let mut scanner =
            Scanner::new(root, ScanSelection::All, &cancellation).expect("fixture scanner starts");
        let mut entries = Vec::new();
        loop {
            let batch = scanner
                .take_batch(limit, &cancellation, FaultInjection::default())
                .expect("fixture batch succeeds");
            let finished = batch.finished;
            entries.extend(batch.entries);
            if finished {
                return entries;
            }
        }
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

    #[test]
    fn scanner_batches_preserve_order_and_positions() {
        let root = tempdir().expect("temporary directory is created");
        std::fs::create_dir(root.path().join("nested")).expect("nested directory is created");
        for path in ["a", "b", "nested/c", "nested/d", "z"] {
            let path = root.path().join(path);
            std::fs::write(path, b"entry").expect("entry is created");
        }

        let one_at_a_time = scan_fixture_in_batches(open_mount(root.path()), 1)
            .into_iter()
            .map(|entry| (entry.stat.path, entry.position, entry.regular))
            .collect::<Vec<_>>();
        let in_batches = scan_fixture_in_batches(open_mount(root.path()), 3)
            .into_iter()
            .map(|entry| (entry.stat.path, entry.position, entry.regular))
            .collect::<Vec<_>>();

        assert_eq!(one_at_a_time, in_batches);
        assert_eq!(
            one_at_a_time
                .iter()
                .map(|entry| entry.1)
                .collect::<Vec<_>>(),
            [0, 1, 2, 3, 4, 5]
        );
    }

    #[test]
    fn scanner_cancellation_finishes_without_emitting_entries() {
        let root = tempdir().expect("temporary directory is created");
        std::fs::write(root.path().join("input"), b"source").expect("source is created");
        let cancellation = CancellationToken::new();
        let mut scanner = Scanner::new(open_mount(root.path()), ScanSelection::All, &cancellation)
            .expect("scanner starts");
        cancellation.cancel();

        let batch = scanner
            .take_batch(SCAN_BATCH_SIZE, &cancellation, FaultInjection::default())
            .expect("cancelled batch succeeds");
        assert!(batch.finished);
        assert!(batch.entries.is_empty());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn scanner_maps_windows_metadata_contract() {
        let root = tempdir().expect("temporary directory is created");
        std::fs::create_dir(root.path().join("nested")).expect("nested directory is created");
        std::fs::write(root.path().join("nested/input.txt"), b"source")
            .expect("source file is created");

        let entries = scan_fixture(open_mount(root.path())).await;
        assert!(entries.iter().all(|entry| !entry.stat.path.contains('\\')));

        let directory = entries
            .iter()
            .find(|entry| entry.stat.path == "nested")
            .expect("nested directory stat exists");
        assert_ne!(
            directory.stat.mode & super::super::fsutil::FileMode::Dir.bits(),
            0
        );
        assert_eq!(directory.stat.mode & 0o777, 0o755);
        assert_eq!(directory.stat.uid, 0);
        assert_eq!(directory.stat.gid, 0);

        let file = entries
            .iter()
            .find(|entry| entry.stat.path == "nested/input.txt")
            .expect("nested file stat exists");
        assert_eq!(file.stat.mode & 0o777, 0o755);
        assert_eq!(file.stat.uid, 0);
        assert_eq!(file.stat.gid, 0);
        assert!(file.stat.mod_time > 0);
    }

    #[tokio::test]
    async fn scanner_emits_regular_and_symlink_metadata() {
        let root = tempdir().expect("temporary directory is created");
        std::fs::write(root.path().join("target"), b"non-empty target")
            .expect("target file is created");
        #[cfg(unix)]
        std::os::unix::fs::symlink("target", root.path().join("link")).expect("symlink is created");

        let entries = scan_fixture(open_mount(root.path())).await;
        let empty = entries
            .iter()
            .find(|entry| entry.stat.path == "target")
            .expect("target file stat exists");
        assert!(empty.regular);
        assert_eq!(
            empty.stat.size,
            i64::try_from(b"non-empty target".len()).expect("target length fits")
        );
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
            assert_eq!(link.stat.linkname, "target");
            assert_eq!(link.stat.size, 0);
        }
    }

    #[tokio::test]
    async fn scanner_applies_literal_followpaths_with_ancestors() {
        let root = tempdir().expect("temporary directory is created");
        std::fs::create_dir(root.path().join("a")).expect("a directory is created");
        std::fs::create_dir(root.path().join("b")).expect("b directory is created");
        std::fs::create_dir(root.path().join("a/subdir")).expect("subdir is created");
        std::fs::write(root.path().join("a/subdir/input"), b"source").expect("input is created");
        std::fs::write(root.path().join("b/other"), b"other").expect("other is created");
        let mut metadata = MetadataMap::new();
        metadata.insert(
            FOLLOW_PATHS_METADATA,
            tonic::metadata::MetadataValue::try_from("a").expect("follow path metadata is valid"),
        );
        let selection = scan_selection(
            &open_mount(root.path()),
            &parse_options(&metadata).expect("follow path options parse"),
        )
        .expect("follow path is accepted");
        let (sender, mut receiver) = mpsc::channel(ENTRY_QUEUE_CAPACITY);
        let scanner = tokio::task::spawn_blocking({
            let root = open_mount(root.path());
            move || {
                scan_entries_with_selection(
                    root,
                    sender,
                    selection,
                    CancellationToken::new(),
                    FaultInjection::default(),
                )
            }
        });
        let mut paths = Vec::new();
        while let Some(event) = receiver.recv().await {
            paths.push(event.expect("selected scan succeeds").stat.path);
        }
        scanner
            .await
            .expect("selected scanner joins")
            .expect("selected scanner succeeds");
        assert_eq!(paths, ["a", "a/subdir", "a/subdir/input"]);
    }

    #[tokio::test]
    async fn scanner_applies_ordered_filters_and_doublestar() {
        let root = tempdir().expect("temporary directory is created");
        std::fs::create_dir_all(root.path().join("a/b")).expect("nested directory is created");
        std::fs::create_dir(root.path().join("other")).expect("other directory is created");
        std::fs::write(root.path().join("a/b/input.txt"), b"input").expect("input is created");
        std::fs::write(root.path().join("a/b/skip.txt"), b"skip").expect("skip is created");
        std::fs::write(root.path().join("other/output.txt"), b"output").expect("output is created");
        let selection = ScanSelection::from_patterns(
            &[String::from("**/*.txt")],
            &[String::from("**/skip.txt")],
        )
        .expect("patterns compile");
        let (sender, mut receiver) = mpsc::channel(ENTRY_QUEUE_CAPACITY);
        let scanner = tokio::task::spawn_blocking({
            let root = open_mount(root.path());
            move || {
                scan_entries_with_selection(
                    root,
                    sender,
                    selection,
                    CancellationToken::new(),
                    FaultInjection::default(),
                )
            }
        });
        let mut paths = Vec::new();
        while let Some(event) = receiver.recv().await {
            paths.push(event.expect("filtered scan succeeds").stat.path);
        }
        scanner
            .await
            .expect("filtered scanner joins")
            .expect("filtered scanner succeeds");
        assert_eq!(
            paths,
            ["a", "a/b", "a/b/input.txt", "other", "other/output.txt"]
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn scanner_resolves_wildcard_and_transitive_followpaths() {
        let root = tempdir().expect("temporary directory is created");
        std::fs::create_dir(root.path().join("dir")).expect("directory is created");
        std::fs::create_dir(root.path().join("target")).expect("target directory is created");
        std::fs::write(root.path().join("target/input"), b"source")
            .expect("target file is created");
        std::os::unix::fs::symlink("../target", root.path().join("dir/link1"))
            .expect("first symlink is created");
        std::os::unix::fs::symlink("dir/link1", root.path().join("link2"))
            .expect("second symlink is created");
        let options = FileSyncOptions {
            follow_paths: vec![String::from("dir/link*"), String::from("link2")],
            ..Default::default()
        };
        let selection =
            scan_selection(&open_mount(root.path()), &options).expect("followpaths resolve");
        let wildcard_only = FileSyncOptions {
            follow_paths: vec![String::from("dir/link*")],
            ..Default::default()
        };
        let wildcard_selection = scan_selection(&open_mount(root.path()), &wildcard_only)
            .expect("wildcard followpath resolves");
        assert!(wildcard_selection.is_selected(Path::new("dir/link1")));
        assert!(wildcard_selection.is_selected(Path::new("target/input")));
        let (sender, mut receiver) = mpsc::channel(ENTRY_QUEUE_CAPACITY);
        let scanner = tokio::task::spawn_blocking({
            let root = open_mount(root.path());
            move || {
                scan_entries_with_selection(
                    root,
                    sender,
                    selection,
                    CancellationToken::new(),
                    FaultInjection::default(),
                )
            }
        });
        let mut paths = Vec::new();
        while let Some(event) = receiver.recv().await {
            paths.push(event.expect("followpaths scan succeeds").stat.path);
        }
        scanner
            .await
            .expect("followpaths scanner joins")
            .expect("followpaths scanner succeeds");
        assert_eq!(
            paths,
            ["dir", "dir/link1", "link2", "target", "target/input"]
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn scanner_preserves_hardlink_topology() {
        let root = tempdir().expect("temporary directory is created");
        std::fs::write(root.path().join("a"), b"shared").expect("source is created");
        std::fs::hard_link(root.path().join("a"), root.path().join("b"))
            .expect("hardlink is created");
        let entries = scan_fixture(open_mount(root.path())).await;
        let first = entries
            .iter()
            .find(|entry| entry.stat.path == "a")
            .expect("first hardlink stat exists");
        let second = entries
            .iter()
            .find(|entry| entry.stat.path == "b")
            .expect("second hardlink stat exists");
        assert!(first.stat.linkname.is_empty());
        assert_eq!(first.stat.size, 6);
        assert_eq!(second.stat.linkname, "a");
        assert_eq!(second.stat.size, 6);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn scanner_reads_supported_xattrs() {
        let root = tempdir().expect("temporary directory is created");
        let file = root.path().join("input");
        std::fs::write(&file, b"source").expect("source is created");
        if let Err(error) = xattr::set(&file, "user.bollard", b"metadata") {
            if error.kind() == std::io::ErrorKind::Unsupported {
                return;
            }
            panic!("xattr setup failed: {error}");
        }
        let entries = scan_fixture(open_mount(root.path())).await;
        let input = entries
            .iter()
            .find(|entry| entry.stat.path == "input")
            .expect("xattr file stat exists");
        assert_eq!(
            input.stat.xattrs.get("user.bollard"),
            Some(&b"metadata".to_vec())
        );
    }

    #[test]
    fn metadata_decodes_encoded_values() {
        let mut metadata = MetadataMap::new();
        metadata.insert(
            DIR_NAME_METADATA,
            tonic::metadata::MetadataValue::try_from("caf%C3%A9")
                .expect("encoded metadata is valid"),
        );
        metadata.insert(
            "dir-name-encoded",
            tonic::metadata::MetadataValue::try_from("1").expect("encoded marker is valid"),
        );
        let options = parse_options(&metadata).expect("metadata decodes");
        assert_eq!(options.dir_name.as_deref(), Some("café"));
    }

    #[test]
    fn scanner_bounds_directory_entry_collection() {
        let root = tempdir().expect("temporary directory is created");
        std::fs::write(root.path().join("a"), b"a").expect("a is created");
        std::fs::write(root.path().join("b"), b"b").expect("b is created");
        let directory = open_mount(root.path());
        let mut budget = ScanBudget {
            inspected: MAX_ENTRIES - 1,
        };
        let cancellation = CancellationToken::new();
        assert_eq!(
            sorted_names(&directory, &mut budget, &cancellation)
                .expect_err("directory budget is enforced")
                .code(),
            tonic::Code::ResourceExhausted
        );
    }

    #[test]
    fn scanner_rejects_invalid_options_and_long_paths() {
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
        validate_options(&metadata).expect("include patterns are supported");

        let mut metadata = MetadataMap::new();
        metadata.insert(
            FOLLOW_PATHS_METADATA,
            tonic::metadata::MetadataValue::try_from("**/*.rs").expect("metadata value is valid"),
        );
        validate_options(&metadata).expect("wildcard followpaths are supported");
        let mut metadata = MetadataMap::new();
        metadata.insert(
            FOLLOW_PATHS_METADATA,
            tonic::metadata::MetadataValue::try_from(".").expect("metadata value is valid"),
        );
        validate_options(&metadata).expect("the whole-tree follow path is supported");

        let root = tempdir().expect("temporary directory is created");
        let mounts = HashMap::from([(String::from("context"), open_mount(root.path()))]);
        assert!(lookup_mount(&mounts, "context").is_ok());
        assert_eq!(
            lookup_mount(&mounts, "missing").unwrap_err().code(),
            tonic::Code::NotFound
        );
        assert_eq!(
            lookup_mount(&mounts, "con").unwrap_err().code(),
            tonic::Code::NotFound
        );
        assert_eq!(
            lookup_mount(&mounts, "Context").unwrap_err().code(),
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
                &mut HashMap::new(),
            )
            .unwrap_err()
            .code(),
            tonic::Code::InvalidArgument
        );

        for value in ["../escape", "/absolute", "foo/../bar"] {
            let mut metadata = MetadataMap::new();
            metadata.insert(
                FOLLOW_PATHS_METADATA,
                tonic::metadata::MetadataValue::try_from(value)
                    .expect("follow path metadata is valid"),
            );
            assert_eq!(
                validate_options(&metadata).unwrap_err().code(),
                tonic::Code::InvalidArgument
            );
        }
    }

    #[test]
    fn followpath_resolution_has_bounded_input_and_depth() {
        let root = tempdir().expect("temporary directory is created");
        let mount = open_mount(root.path());
        let too_many = vec![String::from("missing"); MAX_FOLLOW_PATHS + 1];
        assert_eq!(
            resolve_follow_paths(&mount, &too_many)
                .expect_err("too many followpaths are rejected")
                .code(),
            tonic::Code::ResourceExhausted
        );

        let too_deep = (0..=MAX_FOLLOW_DEPTH)
            .map(|_| "missing")
            .collect::<Vec<_>>()
            .join("/");
        assert_eq!(
            resolve_follow_paths(&mount, &[too_deep])
                .expect_err("deep followpaths are rejected")
                .code(),
            tonic::Code::ResourceExhausted
        );
    }

    #[test]
    fn scan_selection_bounds_resolved_filter_patterns() {
        let root = tempdir().expect("temporary directory is created");
        let options = FileSyncOptions {
            include_patterns: vec![String::from("included"); MAX_FILESYNC_PATTERNS],
            follow_paths: vec![String::from("followed")],
            ..Default::default()
        };
        assert_eq!(
            scan_selection(&open_mount(root.path()), &options)
                .expect_err("effective pattern limit is enforced")
                .code(),
            tonic::Code::ResourceExhausted
        );
    }

    #[test]
    fn normalize_follow_target_clamps_at_mount_root() {
        assert_eq!(
            normalize_follow_target(Path::new("dir"), Path::new("../../target")),
            Some(PathBuf::from("target"))
        );
    }

    #[cfg(windows)]
    #[test]
    fn normalize_follow_target_resolves_drive_absolute_targets() {
        assert_eq!(
            normalize_follow_target(Path::new("dir"), Path::new(r"C:\target\input")),
            Some(PathBuf::from(r"target\input"))
        );
    }

    #[test]
    fn followpath_wildcard_detection_honors_escaped_metacharacters() {
        assert!(contains_wildcard("link*"));
        assert!(contains_wildcard("link[0-9]"));
        assert!(!contains_wildcard("literal"));
        #[cfg(not(windows))]
        assert!(!contains_wildcard(r"literal\*"));
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
    async fn diff_copy_streams_stats_and_transfers_requested_files() {
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
        let data = responses
            .message()
            .await
            .expect("DATA response succeeds")
            .expect("DATA response exists");
        assert_eq!(data, data_packet(0, b"source".to_vec()));
        let eof = responses
            .message()
            .await
            .expect("DATA EOF succeeds")
            .expect("DATA EOF exists");
        assert_eq!(eof, data_packet(0, Vec::new()));
        sender
            .send(fin_packet())
            .await
            .expect("FIN request is accepted");
        let fin = responses
            .message()
            .await
            .expect("FIN response succeeds")
            .expect("FIN response exists");
        assert_eq!(fin.r#type, PacketType::PacketFin as i32);
        assert!(responses.message().await.expect("stream closes").is_none());

        drop(sender);
        let _ = shutdown_sender.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn diff_copy_terminates_empty_files() {
        let root = tempdir().expect("temporary directory is created");
        std::fs::write(root.path().join("empty"), b"").expect("empty file is created");
        let (sender, mut responses, shutdown, server) =
            open_filesync_transfer(open_mount(root.path())).await;
        read_stat_terminator(&mut responses).await;
        sender.send(request_packet(0)).await.expect("request sends");
        assert_eq!(
            responses.message().await.unwrap().unwrap(),
            data_packet(0, Vec::new())
        );
        sender.send(fin_packet()).await.expect("FIN sends");
        assert_eq!(
            responses.message().await.unwrap().unwrap().r#type,
            PacketType::PacketFin as i32
        );
        let _ = shutdown.send(());
        let _ = server.await;
    }

    #[tokio::test]
    async fn diff_copy_preserves_each_file_data_order() {
        let root = tempdir().expect("temporary directory is created");
        std::fs::write(root.path().join("a"), vec![b'a'; FILE_READ_BUFFER_SIZE * 2])
            .expect("a is created");
        std::fs::write(root.path().join("b"), vec![b'b'; FILE_READ_BUFFER_SIZE * 2])
            .expect("b is created");
        let (sender, mut responses, shutdown, server) =
            open_filesync_transfer(open_mount(root.path())).await;
        read_stat_terminator(&mut responses).await;
        sender.send(request_packet(0)).await.expect("request sends");
        sender.send(request_packet(1)).await.expect("request sends");
        sender.send(fin_packet()).await.expect("FIN sends");
        let mut packets = Vec::new();
        let mut eof = HashSet::new();
        while eof.len() < 2 {
            let packet = responses.message().await.unwrap().unwrap();
            if packet.data.is_empty() {
                eof.insert(packet.id);
            }
            packets.push(packet);
        }
        assert_eq!(
            collect_file_data(&packets, 0).unwrap(),
            vec![b'a'; FILE_READ_BUFFER_SIZE * 2]
        );
        assert_eq!(
            collect_file_data(&packets, 1).unwrap(),
            vec![b'b'; FILE_READ_BUFFER_SIZE * 2]
        );
        assert_eq!(
            responses.message().await.unwrap().unwrap().r#type,
            PacketType::PacketFin as i32
        );
        let _ = shutdown.send(());
        let _ = server.await;
    }

    #[tokio::test]
    async fn diff_copy_rejects_duplicate_and_unknown_requests() {
        for duplicate in [true, false] {
            let root = tempdir().expect("temporary directory is created");
            std::fs::write(root.path().join("input"), b"source").expect("source is created");
            let (sender, mut responses, shutdown, server) =
                open_filesync_transfer(open_mount(root.path())).await;
            read_stat_terminator(&mut responses).await;
            sender
                .send(request_packet(if duplicate { 0 } else { 99 }))
                .await
                .expect("request sends");
            if duplicate {
                let _ = responses.message().await.unwrap().unwrap();
                sender
                    .send(request_packet(0))
                    .await
                    .expect("duplicate sends");
            }
            assert_eq!(
                expect_protocol_error(&mut responses).await.code(),
                tonic::Code::InvalidArgument
            );
            let _ = shutdown.send(());
            let _ = server.await;
        }
    }

    #[tokio::test]
    async fn diff_copy_rejects_unexpected_packets() {
        let root = tempdir().expect("temporary directory is created");
        std::fs::write(root.path().join("input"), b"source").expect("source is created");
        let (sender, mut responses, shutdown, server) =
            open_filesync_transfer(open_mount(root.path())).await;
        read_stat_terminator(&mut responses).await;
        sender
            .send(data_packet(0, b"unexpected".to_vec()))
            .await
            .expect("packet sends");
        assert_eq!(
            expect_protocol_error(&mut responses).await.code(),
            tonic::Code::InvalidArgument
        );
        let _ = shutdown.send(());
        let _ = server.await;
    }

    #[tokio::test]
    async fn diff_copy_rejects_early_fin() {
        let root = tempdir().expect("temporary directory is created");
        std::fs::write(root.path().join("input"), b"source").expect("source is created");
        let (sender, mut responses, shutdown, server) = open_filesync_transfer_with_metadata(
            open_mount(root.path()),
            "context",
            FaultInjection {
                delay_scan: true,
                ..Default::default()
            },
        )
        .await;
        sender.send(fin_packet()).await.expect("FIN sends");
        assert_eq!(
            expect_protocol_error(&mut responses).await.code(),
            tonic::Code::FailedPrecondition
        );
        let _ = shutdown.send(());
        let _ = server.await;
    }

    #[tokio::test]
    async fn diff_copy_rejects_input_eof_before_fin() {
        let root = tempdir().expect("temporary directory is created");
        std::fs::write(root.path().join("input"), b"source").expect("source is created");
        let (sender, mut responses, shutdown, server) =
            open_filesync_transfer(open_mount(root.path())).await;
        read_stat_terminator(&mut responses).await;
        drop(sender);
        assert_eq!(
            expect_protocol_error(&mut responses).await.code(),
            tonic::Code::FailedPrecondition
        );
        let _ = shutdown.send(());
        let _ = server.await;
    }

    #[tokio::test]
    async fn diff_copy_reports_worker_failures() {
        for panic_worker in [false, true] {
            let root = tempdir().expect("temporary directory is created");
            std::fs::write(root.path().join("input"), b"source").expect("source is created");
            let (sender, mut responses, shutdown, server) = open_filesync_transfer_with_metadata(
                open_mount(root.path()),
                "context",
                FaultInjection {
                    panic_worker,
                    ..Default::default()
                },
            )
            .await;
            read_stat_terminator(&mut responses).await;
            if !panic_worker {
                std::fs::remove_file(root.path().join("input")).expect("source is removed");
            }
            sender.send(request_packet(0)).await.expect("request sends");
            assert_eq!(
                expect_protocol_error(&mut responses).await.code(),
                tonic::Code::Internal
            );
            let _ = shutdown.send(());
            let _ = server.await;
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn diff_copy_reports_scanner_errors() {
        use std::os::unix::net::UnixListener;

        let root = tempdir().expect("temporary directory is created");
        let _socket =
            UnixListener::bind(root.path().join("socket")).expect("unix socket is created");
        let (_sender, mut responses, shutdown, server) =
            open_filesync_transfer(open_mount(root.path())).await;
        assert_eq!(
            expect_protocol_error(&mut responses).await.code(),
            tonic::Code::InvalidArgument
        );
        let _ = shutdown.send(());
        let _ = server.await;
    }

    #[tokio::test]
    async fn diff_copy_reports_scanner_panics() {
        let root = tempdir().expect("temporary directory is created");
        let (_sender, mut responses, shutdown, server) = open_filesync_transfer_with_metadata(
            open_mount(root.path()),
            "context",
            FaultInjection {
                panic_scanner: true,
                ..Default::default()
            },
        )
        .await;
        assert_eq!(
            expect_protocol_error(&mut responses).await.code(),
            tonic::Code::Internal
        );
        let _ = shutdown.send(());
        let _ = server.await;
    }

    #[tokio::test]
    async fn diff_copy_rejects_unknown_mount_names() {
        let root = tempdir().expect("temporary directory is created");
        let (address, shutdown, server) = start_filesync_server(open_mount(root.path())).await;
        let mut client =
            bollard_buildkit_proto::moby::filesync::v1::file_sync_client::FileSyncClient::connect(
                format!("http://{address}"),
            )
            .await
            .expect("FileSync client connects");
        let mut request = Request::new(stream::empty::<Packet>());
        request.metadata_mut().insert(
            DIR_NAME_METADATA,
            tonic::metadata::MetadataValue::try_from("missing").expect("metadata value is valid"),
        );
        let error = client
            .diff_copy(request)
            .await
            .expect_err("unknown mount names are rejected");
        assert_eq!(error.code(), tonic::Code::NotFound);
        let _ = shutdown.send(());
        let _ = server.await;
    }

    #[tokio::test]
    async fn diff_copy_rejects_non_regular_requests() {
        let root = tempdir().expect("temporary directory is created");
        std::fs::create_dir(root.path().join("directory")).expect("directory is created");
        std::fs::write(root.path().join("directory/input"), b"source").expect("source is created");
        let (sender, mut responses, shutdown, server) =
            open_filesync_transfer(open_mount(root.path())).await;
        let stats = read_stat_terminator(&mut responses).await;
        assert_eq!(stats[0].stat.as_ref().unwrap().path, "directory");
        sender.send(request_packet(0)).await.expect("request sends");
        assert_eq!(
            expect_protocol_error(&mut responses).await.code(),
            tonic::Code::InvalidArgument
        );
        let _ = shutdown.send(());
        let _ = server.await;
    }

    #[tokio::test]
    async fn diff_copy_rejects_unknown_and_unexpected_packet_types() {
        for packet in [
            Packet {
                r#type: 999,
                ..Default::default()
            },
            stat_packet("unexpected"),
            data_packet(0, b"unexpected".to_vec()),
        ] {
            let root = tempdir().expect("temporary directory is created");
            std::fs::write(root.path().join("input"), b"source").expect("source is created");
            let (sender, mut responses, shutdown, server) =
                open_filesync_transfer(open_mount(root.path())).await;
            read_stat_terminator(&mut responses).await;
            sender.send(packet).await.expect("packet sends");
            assert_eq!(
                expect_protocol_error(&mut responses).await.code(),
                tonic::Code::InvalidArgument
            );
            let _ = shutdown.send(());
            let _ = server.await;
        }
    }

    #[tokio::test]
    async fn diff_copy_rejects_repeated_fin_and_peer_errors_during_transfer() {
        for peer_error in [false, true] {
            let root = tempdir().expect("temporary directory is created");
            std::fs::write(
                root.path().join("input"),
                vec![b'x'; FILE_READ_BUFFER_SIZE * 8],
            )
            .expect("source is created");
            let (sender, mut responses, shutdown, server) =
                open_filesync_transfer(open_mount(root.path())).await;
            read_stat_terminator(&mut responses).await;
            sender.send(request_packet(0)).await.expect("request sends");
            sender
                .send(if peer_error {
                    err_packet("peer failure")
                } else {
                    fin_packet()
                })
                .await
                .expect("control packet sends");
            sender
                .send(fin_packet())
                .await
                .expect("second control packet sends");
            assert_eq!(
                expect_protocol_error(&mut responses).await.code(),
                if peer_error {
                    tonic::Code::Aborted
                } else {
                    tonic::Code::FailedPrecondition
                }
            );
            let _ = shutdown.send(());
            let _ = server.await;
        }
    }

    #[tokio::test]
    async fn diff_copy_accepts_requests_after_batched_stats() {
        let root = tempdir().expect("temporary directory is created");
        std::fs::write(root.path().join("a"), b"a").expect("a is created");
        std::fs::write(root.path().join("b"), b"b").expect("b is created");
        let (sender, mut responses, shutdown, server) = open_filesync_transfer_with_metadata(
            open_mount(root.path()),
            "context",
            FaultInjection {
                delay_scan: true,
                ..Default::default()
            },
        )
        .await;
        read_stat_terminator(&mut responses).await;
        sender.send(request_packet(0)).await.expect("request sends");

        let mut data_packets = 0;
        loop {
            let packet = responses
                .message()
                .await
                .expect("FileSync response succeeds")
                .expect("FileSync response exists");
            match PacketType::try_from(packet.r#type).expect("response type is known") {
                PacketType::PacketData => {
                    if packet.data.is_empty() {
                        break;
                    }
                    data_packets += 1;
                }
                PacketType::PacketStat => panic!("STAT terminator was already consumed"),
                _ => {}
            }
        }
        assert!(data_packets > 0);
        sender.send(fin_packet()).await.expect("FIN sends");
        assert_eq!(
            responses.message().await.unwrap().unwrap().r#type,
            PacketType::PacketFin as i32
        );
        let _ = shutdown.send(());
        let _ = server.await;
    }

    #[tokio::test]
    async fn diff_copy_handles_large_stat_streams_with_batched_scanner() {
        let root = tempdir().expect("temporary directory is created");
        for index in 0..(ENTRY_QUEUE_CAPACITY * 2) {
            std::fs::write(root.path().join(format!("entry-{index:03}")), b"entry")
                .expect("entry is created");
        }
        let (sender, mut responses, shutdown, server) =
            open_filesync_transfer(open_mount(root.path())).await;
        let stats = read_stat_terminator(&mut responses).await;
        assert_eq!(stats.len(), ENTRY_QUEUE_CAPACITY * 2 + 1);
        sender.send(fin_packet()).await.expect("FIN sends");
        assert_eq!(
            responses.message().await.unwrap().unwrap().r#type,
            PacketType::PacketFin as i32
        );
        let _ = shutdown.send(());
        let _ = server.await;
    }

    #[tokio::test]
    async fn diff_copy_preserves_pipelined_requests_during_batched_stats() {
        let file_count = SCAN_BATCH_SIZE * 2;
        let root = tempdir().expect("temporary directory is created");
        for index in 0..file_count {
            std::fs::write(root.path().join(format!("entry-{index:03}")), b"entry")
                .expect("entry is created");
        }
        let (sender, mut responses, shutdown, server) = open_filesync_transfer_with_metadata(
            open_mount(root.path()),
            "context",
            FaultInjection {
                delay_scan: true,
                ..Default::default()
            },
        )
        .await;

        let first_stat = tokio::time::timeout(Duration::from_secs(10), responses.message())
            .await
            .expect("first STAT does not stall")
            .expect("first STAT succeeds")
            .expect("first STAT exists");
        assert_eq!(first_stat.r#type, PacketType::PacketStat as i32);
        assert!(first_stat.stat.is_some());
        sender.send(request_packet(0)).await.expect("request sends");

        let mut data_eofs = 0;
        let mut stats_finished = false;
        let mut next_request_id = 1;
        while !stats_finished || data_eofs < file_count {
            let packet = tokio::time::timeout(Duration::from_secs(10), responses.message())
                .await
                .expect("FileSync response does not stall")
                .expect("FileSync response succeeds")
                .expect("FileSync response exists");
            match PacketType::try_from(packet.r#type).expect("response type is known") {
                PacketType::PacketData if packet.data.is_empty() => data_eofs += 1,
                PacketType::PacketStat if packet.stat.is_some() => {
                    if next_request_id < file_count as u32 {
                        let id = next_request_id;
                        next_request_id += 1;
                        sender
                            .send(request_packet(id))
                            .await
                            .expect("request sends");
                    }
                }
                PacketType::PacketStat if packet.stat.is_none() => stats_finished = true,
                _ => {}
            }
        }
        sender.send(fin_packet()).await.expect("FIN sends");
        assert_eq!(
            responses.message().await.unwrap().unwrap().r#type,
            PacketType::PacketFin as i32
        );
        let _ = shutdown.send(());
        let _ = server.await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn diff_copy_rejects_replaced_regular_files() {
        let root = tempdir().expect("temporary directory is created");
        let outside = root.path().with_extension("outside");
        std::fs::write(&outside, b"outside").expect("outside target is created");

        for replacement in ["outside-symlink", "inside-symlink", "directory"] {
            std::fs::write(root.path().join("input"), b"source").expect("source is created");
            let (sender, mut responses, shutdown, server) =
                open_filesync_transfer(open_mount(root.path())).await;
            read_stat_terminator(&mut responses).await;
            std::fs::remove_file(root.path().join("input")).expect("source is removed");
            match replacement {
                "outside-symlink" => {
                    std::os::unix::fs::symlink(&outside, root.path().join("input"))
                        .expect("outside symlink is created")
                }
                "inside-symlink" => {
                    std::os::unix::fs::symlink("replacement-target", root.path().join("input"))
                        .expect("inside symlink is created")
                }
                "directory" => std::fs::create_dir(root.path().join("input"))
                    .expect("replacement directory is created"),
                _ => unreachable!(),
            }
            if replacement == "inside-symlink" {
                std::fs::write(root.path().join("replacement-target"), b"different")
                    .expect("replacement target is created");
            }

            sender.send(request_packet(0)).await.expect("request sends");
            let error = expect_protocol_error(&mut responses).await;
            assert!(
                matches!(
                    error.code(),
                    tonic::Code::Internal | tonic::Code::InvalidArgument
                ),
                "replacement {replacement:?} returned {:?}",
                error.code()
            );
            let _ = shutdown.send(());
            let _ = server.await;
            let input = root.path().join("input");
            if input.is_dir() {
                std::fs::remove_dir(input).expect("replacement directory is removed");
            } else {
                std::fs::remove_file(input).expect("replacement link is removed");
            }
            let target = root.path().join("replacement-target");
            if target.exists() {
                std::fs::remove_file(target).expect("replacement target is removed");
            }
        }
        std::fs::remove_file(outside).expect("outside target is removed");
    }

    #[tokio::test]
    async fn diff_copy_reads_current_file_contents_after_stat() {
        let root = tempdir().expect("temporary directory is created");
        std::fs::write(root.path().join("input"), b"before").expect("source is created");
        let (sender, mut responses, shutdown, server) =
            open_filesync_transfer(open_mount(root.path())).await;
        read_stat_terminator(&mut responses).await;
        std::fs::write(root.path().join("input"), b"after").expect("source is modified");
        sender.send(request_packet(0)).await.expect("request sends");
        assert_eq!(
            responses.message().await.unwrap().unwrap(),
            data_packet(0, b"after".to_vec())
        );
        assert_eq!(
            responses.message().await.unwrap().unwrap(),
            data_packet(0, Vec::new())
        );
        sender.send(fin_packet()).await.expect("FIN sends");
        assert_eq!(
            responses.message().await.unwrap().unwrap().r#type,
            PacketType::PacketFin as i32
        );
        let _ = shutdown.send(());
        let _ = server.await;
    }

    #[tokio::test]
    async fn diff_copy_applies_output_backpressure() {
        let root = tempdir().expect("temporary directory is created");
        std::fs::write(
            root.path().join("input"),
            vec![b'x'; FILE_READ_BUFFER_SIZE * 32],
        )
        .expect("source is created");
        let (sender, mut responses, shutdown, server) =
            open_filesync_transfer(open_mount(root.path())).await;
        read_stat_terminator(&mut responses).await;
        sender.send(request_packet(0)).await.expect("request sends");
        sender.send(fin_packet()).await.expect("FIN sends");
        let mut packets = Vec::new();
        loop {
            let packet = responses.message().await.unwrap().unwrap();
            let done = packet.r#type == PacketType::PacketData as i32 && packet.data.is_empty();
            if packet.r#type == PacketType::PacketData as i32 {
                assert!(packet.data.len() <= FILE_READ_BUFFER_SIZE);
            }
            packets.push(packet);
            if done {
                break;
            }
        }
        assert_eq!(
            collect_file_data(&packets, 0).unwrap().len(),
            FILE_READ_BUFFER_SIZE * 32
        );
        assert_eq!(
            responses.message().await.unwrap().unwrap().r#type,
            PacketType::PacketFin as i32
        );
        let _ = shutdown.send(());
        let _ = server.await;
    }

    #[tokio::test]
    async fn diff_copy_cancellation_stops_blocked_tasks() {
        for backpressured in [false, true] {
            let root = tempdir().expect("temporary directory is created");
            let (sender, mut responses, shutdown, server) = if backpressured {
                std::fs::write(
                    root.path().join("input"),
                    vec![b'x'; FILE_READ_BUFFER_SIZE * OUTPUT_QUEUE_CAPACITY * 2],
                )
                .expect("source is created");
                open_filesync_transfer(open_mount(root.path())).await
            } else {
                for index in 0..16 {
                    std::fs::write(root.path().join(format!("entry-{index}")), b"entry")
                        .expect("entry is created");
                }
                open_filesync_transfer_with_metadata(
                    open_mount(root.path()),
                    "context",
                    FaultInjection {
                        delay_scan: true,
                        ..Default::default()
                    },
                )
                .await
            };
            if backpressured {
                read_stat_terminator(&mut responses).await;
                sender.send(request_packet(0)).await.expect("request sends");
            } else {
                let _ = responses.message().await;
            }
            drop(sender);
            drop(responses);
            let _ = shutdown.send(());
            tokio::time::timeout(std::time::Duration::from_secs(1), server)
                .await
                .expect("blocked FileSync service shuts down")
                .expect("blocked FileSync service joins");
        }
    }
}
