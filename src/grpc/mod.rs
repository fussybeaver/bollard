//! GRPC plumbing to interact with Docker's buildkit client
#![cfg(feature = "buildkit_providerless")]
#![allow(dead_code)]

/// End-user buildkit build functions
pub mod build;
/// A package of GRPC buildkit connection implementations
pub mod driver;
/// Errors for the GRPC modules
pub mod error;
/// End-user buildkit export functions
pub mod export;
mod fsutil;
/// Internal interfaces to convert types for GRPC communication
pub(crate) mod io;
/// End-user buildkit registry functions
pub mod registry;
mod ssh;

use crate::auth::DockerCredentials;
use crate::docker::BodyType;
use crate::health::health_check_response::ServingStatus;
use crate::health::health_server::Health;
use crate::health::{HealthCheckRequest, HealthCheckResponse};
use crate::moby::filesync::v1::auth_server::Auth;
use crate::moby::filesync::v1::file_send_server::FileSend;
use crate::moby::filesync::v1::{
    BytesMessage as FileSyncBytesMessage, CredentialsRequest, CredentialsResponse,
    FetchTokenRequest, FetchTokenResponse, GetTokenAuthorityRequest, GetTokenAuthorityResponse,
    VerifyTokenAuthorityRequest, VerifyTokenAuthorityResponse,
};
use crate::moby::upload::v1::upload_server::{Upload, UploadServer};
use crate::moby::upload::v1::BytesMessage as UploadBytesMessage;
use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsString;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll};

use bollard_buildkit_proto::fsutil::types::packet::PacketType;
use bollard_buildkit_proto::fsutil::types::{Packet, Stat};
use bollard_buildkit_proto::health::{HealthListRequest, HealthListResponse};
use bollard_buildkit_proto::moby::buildkit::secrets::v1::secrets_server::{Secrets, SecretsServer};
use bollard_buildkit_proto::moby::buildkit::secrets::v1::{GetSecretRequest, GetSecretResponse};
use bollard_buildkit_proto::moby::filesync::packet::file_send_server::{
    FileSend as FileSendPacket, FileSendServer as FileSendPacketServer,
};
use bollard_buildkit_proto::moby::filesync::v1::auth_server::AuthServer;
use bollard_buildkit_proto::moby::filesync::v1::file_send_server::FileSendServer;
use bollard_buildkit_proto::moby::sshforward::v1::ssh_server::{Ssh, SshServer};
use bollard_buildkit_proto::moby::sshforward::v1::{CheckAgentRequest, CheckAgentResponse};
use bytes::Bytes;
use error::GrpcSshError;
use futures_core::Stream;
use http_body_util::{BodyExt, Full};
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use log::{debug, error, info, trace, warn};
use rustls::ALL_VERSIONS;
use serde_derive::Deserialize;
use ssh::SshAgentPacketDecoder;
use tokio::sync::mpsc;
use tokio_util::codec::FramedRead;
use tokio_util::io::{ReaderStream, StreamReader};
use tonic::server::NamedService;
use tonic::{Code, Request, Response, Status, Streaming};

use futures_util::{StreamExt, TryFutureExt};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;

use http::request::Builder;
use hyper::Method;
use std::future::Future;
use tower_service::Service;

use self::error::GrpcAuthError;
use self::io::GrpcTransport;

// Datetime handling with cfg guards for time/chrono feature parity
// BuildKit OAuth requires either 'time' or 'chrono' feature for RFC3339 parsing
#[cfg(not(any(feature = "time", feature = "chrono")))]
compile_error!(
    "BuildKit requires either 'time' or 'chrono' feature to be enabled for OAuth authentication"
);

#[cfg(feature = "time")]
type GrpcDateTime = time::OffsetDateTime;

#[cfg(all(feature = "chrono", not(feature = "time")))]
type GrpcDateTime = chrono::DateTime<chrono::Utc>;

#[cfg(feature = "time")]
fn grpc_now() -> GrpcDateTime {
    time::OffsetDateTime::now_utc()
}

#[cfg(all(feature = "chrono", not(feature = "time")))]
fn grpc_now() -> GrpcDateTime {
    chrono::Utc::now()
}

#[cfg(feature = "time")]
fn grpc_timestamp(dt: &GrpcDateTime) -> i64 {
    dt.unix_timestamp()
}

#[cfg(all(feature = "chrono", not(feature = "time")))]
fn grpc_timestamp(dt: &GrpcDateTime) -> i64 {
    dt.timestamp()
}

const MAX_SECRET_SIZE: u64 = 500 * 1024; // 500KB

#[derive(Debug)]
pub(crate) enum GrpcServer {
    Auth(AuthServer<AuthProvider>),
    Upload(UploadServer<UploadProvider>),
    FileSend(FileSendServer<FileSendImpl>),
    FileSendPacket(FileSendPacketServer<FileSendPacketImpl>),
    Secrets(SecretsServer<SecretProvider>),
    Ssh(SshServer<SshProvider>),
}

impl GrpcServer {
    pub(crate) fn append(
        self,
        builder: tonic::transport::server::Router,
    ) -> tonic::transport::server::Router {
        match self {
            GrpcServer::Auth(auth_server) => builder.add_service(auth_server),
            GrpcServer::Upload(upload_server) => builder.add_service(upload_server),
            GrpcServer::FileSend(file_send_server) => builder.add_service(file_send_server),
            GrpcServer::FileSendPacket(file_send_packet_server) => {
                builder.add_service(file_send_packet_server)
            }
            GrpcServer::Secrets(secret_server) => builder.add_service(secret_server),
            GrpcServer::Ssh(ssh_server) => builder.add_service(ssh_server),
        }
    }

    /// Internal name published as part of the GRPC communication
    pub fn names(&self) -> Vec<String> {
        match self {
            GrpcServer::Auth(_auth_server) => {
                vec![
                    format!("/{}/credentials", AuthServer::<AuthProvider>::NAME),
                    format!("/{}/fetch_token", AuthServer::<AuthProvider>::NAME),
                ]
            }
            GrpcServer::Upload(_upload_server) => {
                vec![format!("/{}/pull", UploadServer::<UploadProvider>::NAME)]
            }
            GrpcServer::FileSend(_file_send_server) => {
                vec![format!(
                    "/{}/diffcopy",
                    FileSendServer::<FileSendImpl>::NAME
                )]
            }
            GrpcServer::FileSendPacket(_file_send_packet_server) => {
                vec![format!(
                    "/{}/diffcopy",
                    FileSendPacketServer::<FileSendPacketImpl>::NAME
                )]
            }
            GrpcServer::Secrets(_secret_server) => {
                vec![format!(
                    "/{}/GetSecret",
                    SecretsServer::<SecretProvider>::NAME
                )]
            }
            GrpcServer::Ssh(_ssh_server) => {
                vec![
                    format!("/{}/CheckAgent", SshServer::<SshProvider>::NAME),
                    format!("/{}/ForwardAgent", SshServer::<SshProvider>::NAME),
                ]
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct HealthServerImpl {
    service_map: HashMap<String, ServingStatus>,
    shutdown: bool,
}

impl HealthServerImpl {
    pub fn new() -> Self {
        let mut service_map = HashMap::new();
        service_map.insert(String::from(""), ServingStatus::Serving);
        Self {
            service_map,
            shutdown: false,
        }
    }

    pub fn shutdown(mut self) {
        self.shutdown = true;
        for (_, val) in self.service_map.iter_mut() {
            *val = ServingStatus::NotServing;
        }
    }
}

#[tonic::async_trait]
impl Health for HealthServerImpl {
    type WatchStream = Pin<Box<dyn Stream<Item = Result<HealthCheckResponse, Status>> + Send>>;
    async fn check(
        &self,
        request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        trace!("Received GRPC Health Request: {:#?}", request);
        if let Some(status) = self.service_map.get(&request.get_ref().service) {
            Ok(Response::new(HealthCheckResponse {
                status: *status as i32,
            }))
        } else {
            Err(Status::new(Code::NotFound, "unknown service"))
        }
    }

    async fn list(
        &self,
        _: tonic::Request<HealthListRequest>,
    ) -> Result<tonic::Response<HealthListResponse>, tonic::Status> {
        unimplemented!()
    }

    #[allow(clippy::diverging_sub_expression)]
    async fn watch(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<Self::WatchStream>, Status> {
        unimplemented!();
    }
}

#[derive(Clone, Debug)]
pub(crate) struct FileSendImpl {
    pub(crate) dest: PathBuf,
}

impl FileSendImpl {
    pub fn new(dest: &Path) -> Self {
        Self {
            dest: dest.to_owned(),
        }
    }
}

#[tonic::async_trait]
impl FileSend for FileSendImpl {
    type DiffCopyStream = Pin<Box<dyn Stream<Item = Result<FileSyncBytesMessage, Status>> + Send>>;
    async fn diff_copy(
        &self,
        request: Request<Streaming<FileSyncBytesMessage>>,
    ) -> Result<Response<Self::DiffCopyStream>, Status> {
        trace!("Protobuf FileSend diff_copy triggered: {:#?}", request);

        let path = self.dest.as_path();

        let mut in_stream = request.into_inner();

        let mut file = tokio::fs::File::create(path).await?;

        while let Some(result) = in_stream.next().await {
            match result {
                Ok(v) => {
                    file.write_all(&v.data).await?;
                }
                Err(err) => return Err(err),
            }
        }

        Ok(Response::new(Box::pin(futures_util::stream::empty())))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct FileSendPacketImpl {
    pub(crate) dest: PathBuf,
    pub(crate) limits: FileTransferLimits,
}

impl FileSendPacketImpl {
    pub fn new(dest: &Path) -> Self {
        Self::with_limits(dest, FileTransferLimits::default())
    }

    pub(crate) fn with_limits(dest: &Path, limits: FileTransferLimits) -> Self {
        Self {
            dest: dest.to_owned(),
            limits,
        }
    }

    fn validate_path(path: &str) -> Result<PathBuf, Status> {
        let relative = Path::new(path);
        if path.is_empty()
            || path.len() > MAX_PATH_LENGTH
            || path.contains('\0')
            || !relative
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_)))
        {
            return Err(Status::invalid_argument(format!(
                "invalid export path: {path:?}"
            )));
        }

        Ok(relative.to_owned())
    }

    fn validate_linkname(linkname: &str) -> Result<(), Status> {
        if linkname.is_empty() {
            return Err(Status::invalid_argument("symlink without target"));
        }
        if linkname.len() > MAX_LINKNAME_LENGTH || linkname.contains('\0') {
            return Err(Status::invalid_argument(
                "symlink target is too long or invalid",
            ));
        }
        Ok(())
    }

    fn validate_stat(stat: &Stat) -> Result<(PathBuf, fsutil::FileMode), Status> {
        let path = Self::validate_path(&stat.path)?;
        if stat.size < 0 {
            return Err(Status::invalid_argument("file stat has a negative size"));
        }
        if u64::try_from(stat.size).unwrap_or(u64::MAX) > MAX_FILE_SIZE {
            return Err(Status::resource_exhausted("file exceeds the maximum size"));
        }

        let mode = fsutil::FileMode::from_bits_truncate(stat.mode);
        let type_bits = stat.mode & fsutil::FileMode::Type.bits();
        if type_bits.count_ones() > 1 {
            return Err(Status::invalid_argument(
                "file stat has conflicting file types",
            ));
        }
        if mode.contains(fsutil::FileMode::Symlink) {
            Self::validate_linkname(&stat.linkname)?;
        } else if !mode.intersects(fsutil::FileMode::Type) && !stat.linkname.is_empty() {
            return Err(Status::invalid_argument(
                "regular file has an unexpected symlink target",
            ));
        }

        Ok((path, mode))
    }
}

/// Aggregate limits for one packet-based local export.
#[derive(Clone, Copy, Debug, Default)]
pub struct FileTransferLimits {
    /// Maximum number of filesystem entries accepted in one transfer.
    pub max_files: Option<u64>,
    /// Maximum sum of declared entry sizes accepted in one transfer.
    pub max_bytes: Option<u64>,
}

const MAX_PATH_LENGTH: usize = 4096;
const MAX_LINKNAME_LENGTH: usize = 4096;
const MAX_FILE_COUNT: usize = 100_000;
const MAX_PENDING_FILES: usize = 4096;
const MAX_FILE_SIZE: u64 = 4 * 1024 * 1024 * 1024;
const MAX_TOTAL_SIZE: u64 = 16 * 1024 * 1024 * 1024;

struct FileReceiveState {
    root: cap_std::fs::Dir,
    stats: HashMap<u32, PendingFile>,
    declared_paths: HashSet<PathBuf>,
    directories: HashMap<PathBuf, (u32, cap_std::fs::Dir, OsString)>,
    received_all_stats: bool,
    received_fin: bool,
    next_stat_id: u32,
    file_count: usize,
    total_size: u64,
    limits: FileTransferLimits,
}

struct PendingFile {
    stat: Stat,
    file: File,
    received_bytes: u64,
}

impl FileReceiveState {
    async fn new(base: PathBuf) -> Result<Self, Status> {
        Self::with_limits(base, FileTransferLimits::default()).await
    }

    async fn with_limits(base: PathBuf, limits: FileTransferLimits) -> Result<Self, Status> {
        let root = tokio::task::spawn_blocking(move || {
            cap_std::fs::Dir::open_ambient_dir(&base, cap_std::ambient_authority())
        })
        .await
        .map_err(|error| {
            Status::internal(format!(
                "filesystem worker failed while opening export directory: {error}"
            ))
        })?;
        let root = root.map_err(|error| {
            Status::internal(format!("failed to open export directory: {error}"))
        })?;

        Ok(Self {
            root,
            stats: HashMap::new(),
            declared_paths: HashSet::new(),
            directories: HashMap::new(),
            received_all_stats: false,
            received_fin: false,
            next_stat_id: 0,
            file_count: 0,
            total_size: 0,
            limits,
        })
    }

    fn is_complete(&self) -> bool {
        self.received_all_stats && self.stats.is_empty()
    }

    async fn handle_packet(&mut self, packet: Packet) -> Result<Option<Packet>, Status> {
        if self.received_fin {
            return Err(Status::failed_precondition(
                "packet received after PACKET_FIN",
            ));
        }

        let packet_type = PacketType::try_from(packet.r#type)
            .map_err(|_| Status::invalid_argument("unknown packet type"))?;

        match packet_type {
            PacketType::PacketStat => {
                if let Some(stat) = packet.stat {
                    if self.received_all_stats {
                        return Err(Status::failed_precondition(
                            "file stat received after terminating stat packet",
                        ));
                    }
                    let request_id = self.next_stat_id;
                    self.next_stat_id = self.next_stat_id.checked_add(1).ok_or_else(|| {
                        Status::resource_exhausted("file stat request ID exhausted")
                    })?;
                    let needs_data = self.receive_stat(request_id, &stat).await?;

                    if needs_data {
                        Ok(Some(Packet {
                            r#type: PacketType::PacketReq.into(),
                            stat: None,
                            id: request_id,
                            data: vec![],
                        }))
                    } else {
                        Ok(None)
                    }
                } else {
                    if self.received_all_stats {
                        return Err(Status::failed_precondition(
                            "duplicate terminating stat packet",
                        ));
                    }

                    self.received_all_stats = true;
                    if self.is_complete() {
                        Ok(Some(Self::fin_packet()))
                    } else {
                        Ok(None)
                    }
                }
            }
            PacketType::PacketReq => Err(Status::failed_precondition(
                "server received a request packet",
            )),
            PacketType::PacketData => {
                if packet.data.is_empty() {
                    self.finish_file(packet.id).await?;
                    if self.is_complete() {
                        Ok(Some(Self::fin_packet()))
                    } else {
                        Ok(None)
                    }
                } else {
                    self.append_file(packet.id, &packet.data).await?;
                    Ok(None)
                }
            }
            PacketType::PacketFin => {
                if !self.is_complete() {
                    return Err(Status::failed_precondition(
                        "file transfer finished before all files were received",
                    ));
                }
                self.received_fin = true;
                Ok(None)
            }
            PacketType::PacketErr => {
                let message = String::from_utf8_lossy(&packet.data);
                Err(Status::unknown(format!("packet error: {message}")))
            }
        }
    }

    fn fin_packet() -> Packet {
        Packet {
            r#type: PacketType::PacketFin.into(),
            stat: None,
            id: 0,
            data: vec![],
        }
    }

    async fn ensure_parent_dirs(
        &mut self,
        path: &Path,
    ) -> Result<(cap_std::fs::Dir, Vec<(PathBuf, cap_std::fs::Dir, OsString)>), Status> {
        let parent = path.parent().unwrap_or_else(|| Path::new(""));
        let root = self.root.try_clone().map_err(|error| {
            Status::internal(format!("failed to retain export directory: {error}"))
        })?;
        let parent = parent.to_owned();

        tokio::task::spawn_blocking(move || {
            let mut current = root;
            let mut relative = PathBuf::new();
            let mut created = Vec::new();

            for component in parent.components() {
                let name = component.as_os_str();
                relative.push(name);
                match current.open_dir(name) {
                    Ok(next) => current = next,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        let permission_parent = current.try_clone()?;
                        let permission_name = name.to_owned();
                        let mut builder = cap_std::fs::DirBuilder::new();
                        #[cfg(unix)]
                        {
                            use cap_std::fs::DirBuilderExt;
                            builder.mode(0o700);
                        }
                        current.create_dir_with(name, &builder).map_err(|error| {
                            std::io::Error::new(
                                error.kind(),
                                format!("failed to create export directory: {error}"),
                            )
                        })?;
                        current = current.open_dir(name)?;
                        created.push((relative.clone(), permission_parent, permission_name));
                    }
                    Err(error) => return Err(error),
                }
            }

            Ok((current, created))
        })
        .await
        .map_err(|error| Status::internal(format!("filesystem worker failed: {error}")))?
        .map_err(|error| Status::invalid_argument(format!("unsafe export path: {error}")))
    }

    async fn receive_stat(&mut self, request_id: u32, stat: &Stat) -> Result<bool, Status> {
        let declared_size = u64::try_from(stat.size)
            .map_err(|_| Status::invalid_argument("file stat has a negative size"))?;
        if let Some(max_files) = self.limits.max_files {
            if self.file_count as u64 >= max_files {
                return Err(Status::resource_exhausted(
                    "file transfer exceeds the maximum entry count",
                ));
            }
        }
        if let Some(max_bytes) = self.limits.max_bytes {
            let total = self
                .total_size
                .checked_add(declared_size)
                .ok_or_else(|| Status::resource_exhausted("file transfer byte count overflow"))?;
            if total > max_bytes {
                return Err(Status::resource_exhausted(
                    "file transfer exceeds the maximum byte count",
                ));
            }
        }
        if self.file_count >= MAX_FILE_COUNT {
            return Err(Status::resource_exhausted("too many files in export"));
        }
        let (path, mode) = FileSendPacketImpl::validate_stat(stat)?;
        if !self.declared_paths.insert(path.clone()) {
            return Err(Status::already_exists(format!(
                "duplicate export path: {:?}",
                stat.path
            )));
        }

        self.total_size = self
            .total_size
            .checked_add(declared_size)
            .ok_or_else(|| Status::resource_exhausted("export size overflow"))?;
        if self.total_size > MAX_TOTAL_SIZE {
            return Err(Status::resource_exhausted(
                "export exceeds the maximum size",
            ));
        }

        let (parent, created) = self.ensure_parent_dirs(&path).await?;
        for (directory, parent, name) in created {
            self.directories
                .entry(directory)
                .or_insert((0o700, parent, name));
        }

        let name = path.file_name().ok_or_else(|| {
            Status::invalid_argument(format!("export path has no filename: {:?}", stat.path))
        })?;

        if mode.contains(fsutil::FileMode::Symlink) {
            #[cfg(unix)]
            tokio::task::spawn_blocking({
                let linkname = stat.linkname.clone();
                let parent = parent.try_clone().map_err(|error| {
                    Status::internal(format!("failed to retain export directory: {error}"))
                })?;
                let name = name.to_owned();
                move || parent.symlink_contents(linkname, name)
            })
            .await
            .map_err(|error| Status::internal(format!("filesystem worker failed: {error}")))?
            .map_err(|error| Status::internal(format!("failed to create symlink: {error}")))?;
            #[cfg(not(unix))]
            return Err(Status::unimplemented(
                "symlink export is only supported on Unix",
            ));
            self.file_count += 1;
            return Ok(false);
        }

        if mode.contains(fsutil::FileMode::Dir) {
            let permission_parent = parent.try_clone().map_err(|error| {
                Status::internal(format!("failed to retain export directory: {error}"))
            })?;
            let permission_name = name.to_owned();
            let parent = parent.try_clone().map_err(|error| {
                Status::internal(format!("failed to retain export directory: {error}"))
            })?;
            let name = name.to_owned();
            let directory = tokio::task::spawn_blocking(move || match parent.open_dir(&name) {
                Ok(directory) => Ok(directory),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    let mut builder = cap_std::fs::DirBuilder::new();
                    #[cfg(unix)]
                    {
                        use cap_std::fs::DirBuilderExt;
                        builder.mode(0o700);
                    }
                    parent.create_dir_with(&name, &builder)?;
                    parent.open_dir(&name)
                }
                Err(error) => Err(error),
            })
            .await
            .map_err(|error| Status::internal(format!("filesystem worker failed: {error}")))?
            .map_err(|error| {
                Status::already_exists(format!("cannot create export directory: {error}"))
            })?;
            drop(directory);
            self.directories.insert(
                path,
                (stat.mode & 0o777, permission_parent, permission_name),
            );
            self.file_count += 1;
            return Ok(false);
        }

        if mode.intersects(fsutil::FileMode::Type) {
            return Err(Status::unimplemented(format!(
                "unsupported file type for path {:?}",
                stat.path
            )));
        }

        if self.stats.len() >= MAX_PENDING_FILES {
            return Err(Status::resource_exhausted(
                "too many files are pending data",
            ));
        }

        let parent = parent.try_clone().map_err(|error| {
            Status::internal(format!("failed to retain export directory: {error}"))
        })?;
        let name = name.to_owned();
        let file = tokio::task::spawn_blocking(move || {
            let mut options = cap_std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use cap_std::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            parent
                .open_with(&name, &options)
                .map(|file| file.into_std())
        })
        .await
        .map_err(|error| Status::internal(format!("filesystem worker failed: {error}")))?
        .map_err(|error| Status::already_exists(format!("cannot create export file: {error}")))?;

        self.file_count += 1;
        self.stats.insert(
            request_id,
            PendingFile {
                stat: stat.clone(),
                file: File::from_std(file),
                received_bytes: 0,
            },
        );
        Ok(true)
    }

    async fn append_file(&mut self, id: u32, data: &[u8]) -> Result<(), Status> {
        let pending = self
            .stats
            .get_mut(&id)
            .ok_or_else(|| Status::invalid_argument("data packet for unknown file"))?;
        let data_len = u64::try_from(data.len())
            .map_err(|_| Status::resource_exhausted("file packet size overflow"))?;
        let received_bytes = pending
            .received_bytes
            .checked_add(data_len)
            .ok_or_else(|| Status::resource_exhausted("file byte count overflow"))?;
        let expected_bytes = u64::try_from(pending.stat.size)
            .map_err(|_| Status::invalid_argument("file stat has a negative size"))?;
        if received_bytes > expected_bytes {
            return Err(Status::resource_exhausted(
                "file transfer exceeds the declared file size",
            ));
        }
        pending
            .file
            .write_all(data)
            .await
            .map_err(|error| Status::internal(format!("failed to write file data: {error}")))?;
        pending.received_bytes = received_bytes;
        Ok(())
    }

    async fn finish_file(&mut self, id: u32) -> Result<(), Status> {
        let mut pending = self
            .stats
            .remove(&id)
            .ok_or_else(|| Status::invalid_argument("end-of-file packet for unknown file"))?;
        let expected_bytes = u64::try_from(pending.stat.size)
            .map_err(|_| Status::invalid_argument("file stat has a negative size"))?;
        if pending.received_bytes != expected_bytes {
            return Err(Status::invalid_argument(format!(
                "file ended after {} of {} bytes",
                pending.received_bytes, expected_bytes
            )));
        }

        pending
            .file
            .flush()
            .await
            .map_err(|error| Status::internal(format!("failed to flush file data: {error}")))?;
        #[cfg(unix)]
        pending
            .file
            .set_permissions(std::fs::Permissions::from_mode(pending.stat.mode & 0o777))
            .await
            .map_err(|error| {
                Status::internal(format!("failed to set file permissions: {error}"))
            })?;
        Ok(())
    }

    async fn finalize(mut self) -> Result<(), Status> {
        if !self.is_complete() {
            return Err(Status::failed_precondition(
                "file transfer finalized before all files were received",
            ));
        }
        self.stats.clear();
        let directories: Vec<_> = self.directories.into_values().collect();
        tokio::task::spawn_blocking(move || {
            for (mode, parent, name) in directories {
                #[cfg(unix)]
                parent.set_permissions(
                    name,
                    cap_std::fs::Permissions::from_std(std::fs::Permissions::from_mode(mode)),
                )?;
                #[cfg(not(unix))]
                let _ = (parent, name, mode);
            }
            Ok::<(), std::io::Error>(())
        })
        .await
        .map_err(|error| Status::internal(format!("filesystem worker failed: {error}")))?
        .map_err(|error| Status::internal(format!("failed to set directory permissions: {error}")))
    }

    fn finish_stream(&self) -> Result<(), Status> {
        if self.received_fin {
            Ok(())
        } else {
            Err(Status::failed_precondition(
                "file packet stream ended before PACKET_FIN",
            ))
        }
    }
}

async fn prepare_staging_directory(destination: &Path) -> Result<PathBuf, Status> {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .await
        .map_err(|error| Status::internal(format!("failed to create export parent: {error}")))?;
    let name = destination
        .file_name()
        .ok_or_else(|| Status::invalid_argument("export destination has no filename"))?
        .to_string_lossy();
    let staging = parent.join(format!(".{name}.bollard-staging-{}", crate::grpc::new_id()));
    fs::create_dir(&staging).await.map_err(|error| {
        Status::internal(format!("failed to create staging directory: {error}"))
    })?;
    Ok(staging)
}

struct StagingGuard {
    staging: Option<PathBuf>,
    runtime: tokio::runtime::Handle,
}

impl StagingGuard {
    async fn new(destination: &Path) -> Result<Self, Status> {
        Ok(Self {
            staging: Some(prepare_staging_directory(destination).await?),
            runtime: tokio::runtime::Handle::current(),
        })
    }

    fn path(&self) -> &Path {
        self.staging
            .as_deref()
            .expect("staging guard owns a path before publication")
    }

    async fn cleanup(&mut self) -> Result<(), Status> {
        if let Some(staging) = self.staging.take() {
            remove_path(&staging).await
        } else {
            Ok(())
        }
    }

    async fn publish(&mut self, destination: &Path) -> Result<(), Status> {
        let staging = self
            .staging
            .take()
            .expect("staging guard owns a path before publication");
        publish_staging_directory(&staging, destination).await
    }
}

impl Drop for StagingGuard {
    fn drop(&mut self) {
        let Some(staging) = self.staging.take() else {
            return;
        };

        let runtime = self.runtime.clone();
        runtime.spawn(async move {
            if let Err(error) = remove_path(&staging).await {
                warn!("failed to clean up cancelled FileSend staging directory: {error}");
            }
        });
    }
}

fn remove_path_blocking(path: &Path) -> Result<(), Status> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(Status::internal(format!(
                "failed to inspect cleanup path: {error}"
            )))
        }
    };
    let result = if metadata.is_dir() && !metadata.file_type().is_symlink() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    result.map_err(|error| Status::internal(format!("failed to clean up export path: {error}")))
}

async fn remove_path(path: &Path) -> Result<(), Status> {
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || remove_path_blocking(&path))
        .await
        .map_err(|error| Status::internal(format!("filesystem cleanup worker failed: {error}")))?
}

fn publish_staging_directory_blocking(staging: &Path, destination: &Path) -> Result<(), Status> {
    let result = (|| {
        let parent = destination.parent().unwrap_or_else(|| Path::new("."));
        let name = destination
            .file_name()
            .ok_or_else(|| Status::invalid_argument("export destination has no filename"))?
            .to_string_lossy();
        let backup = parent.join(format!(".{name}.bollard-backup-{}", crate::grpc::new_id()));
        let destination_exists = match std::fs::symlink_metadata(destination) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => {
                return Err(Status::internal(format!(
                    "failed to inspect export destination: {error}"
                )))
            }
        };

        if destination_exists {
            std::fs::rename(destination, &backup).map_err(|error| {
                Status::internal(format!("failed to stage existing export: {error}"))
            })?;
        }

        if let Err(error) = std::fs::rename(staging, destination) {
            if destination_exists {
                if let Err(rollback_error) = std::fs::rename(&backup, destination) {
                    error!(
                        "failed to publish export and roll back destination: publish={error}; rollback={rollback_error}"
                    );
                }
            }
            return Err(Status::internal(format!(
                "failed to publish export: {error}"
            )));
        }

        if destination_exists {
            if let Err(error) = remove_path_blocking(&backup) {
                warn!("published export but failed to remove backup: {error}");
            }
        }
        Ok(())
    })();

    if result.is_err() {
        if let Err(cleanup_error) = remove_path_blocking(staging) {
            warn!("failed to clean up unpublished export: {cleanup_error}");
        }
    }
    result
}

async fn publish_staging_directory(staging: &Path, destination: &Path) -> Result<(), Status> {
    let staging = staging.to_owned();
    let destination = destination.to_owned();
    tokio::task::spawn_blocking(move || publish_staging_directory_blocking(&staging, &destination))
        .await
        .map_err(|error| {
            Status::internal(format!("filesystem publication worker failed: {error}"))
        })?
}

#[tonic::async_trait]
impl FileSendPacket for FileSendPacketImpl {
    type DiffCopyStream = Pin<Box<dyn Stream<Item = Result<Packet, Status>> + Send>>;
    async fn diff_copy(
        &self,
        request: Request<Streaming<Packet>>,
    ) -> Result<Response<Self::DiffCopyStream>, Status> {
        let destination = self.dest.clone();
        let limits = self.limits;
        let mut in_stream = request.into_inner();

        // protocol reference: https://github.com/tonistiigi/fsutil/blob/91a3fc46842c58b62dd4630b688662842364da49/receive.go#L1-L15
        let out_stream = async_stream::try_stream! {
            debug!("starting FileSend packet export");
            let mut staging_guard = StagingGuard::new(&destination).await?;
            let staging = staging_guard.path().to_owned();
            let mut state = Some(match FileReceiveState::with_limits(staging.clone(), limits).await {
                Ok(state) => state,
                Err(error) => {
                    if let Err(cleanup_error) = staging_guard.cleanup().await {
                        warn!("failed to clean up staging directory: {cleanup_error}");
                    }
                    Err::<FileReceiveState, Status>(error)?;
                    unreachable!();
                }
            });

            let mut receiver_sent_fin = false;
            loop {
                match in_stream.next().await {
                    Some(Ok(packet)) => {
                        if receiver_sent_fin {
                            if packet.r#type != PacketType::PacketFin as i32 {
                                Err::<(), Status>(Status::failed_precondition(
                                    "packet received after PACKET_FIN",
                                    ))?;
                            }
                            break;
                        }

                        let packet_result = state
                            .as_mut()
                            .expect("receiver state is present before PACKET_FIN")
                            .handle_packet(packet)
                            .await;
                        match packet_result {
                            Ok(Some(out)) => {
                                if out.r#type == PacketType::PacketFin as i32 {
                                    let completed_state = state
                                        .take()
                                        .expect("receiver state is present before finalization");
                                    if let Err(error) = completed_state.finalize().await {
                                        if let Err(cleanup_error) = staging_guard.cleanup().await {
                                            warn!("failed to clean up unfinalized export: {cleanup_error}");
                                        }
                                        Err::<(), Status>(error)?;
                                    }

                                    if let Err(error) = staging_guard.publish(&destination).await {
                                        Err::<(), Status>(error)?;
                                    }

                                    receiver_sent_fin = true;
                                    yield out;
                                } else {
                                    yield out;
                                }
                            }
                            Ok(None) => {}
                            Err(error) => {
                                if let Err(cleanup_error) = staging_guard.cleanup().await {
                                    warn!("failed to clean up failed export: {cleanup_error}");
                                }
                                Err::<(), Status>(error)?;
                            }
                        }
                    }
                    Some(Err(error)) => {
                        if receiver_sent_fin {
                            warn!("packet stream ended after export publish: {error}");
                            break;
                        }
                        if let Err(cleanup_error) = staging_guard.cleanup().await {
                            warn!("failed to clean up failed export: {cleanup_error}");
                        }
                        Err::<(), Status>(Status::internal(format!("packet stream error: {error}")))?;
                    }
                    None => {
                        if receiver_sent_fin {
                            warn!("packet stream ended after export publish");
                        } else {
                            if let Err(cleanup_error) = staging_guard.cleanup().await {
                                warn!("failed to clean up incomplete export: {cleanup_error}");
                            }
                            Err::<(), Status>(Status::failed_precondition(
                                "file packet stream ended before PACKET_FIN",
                            ))?;
                        }
                        break;
                    }
                }
            }

            if !receiver_sent_fin {
                if let Err(cleanup_error) = staging_guard.cleanup().await {
                    warn!("failed to clean up incomplete export: {cleanup_error}");
                }
                Err::<(), Status>(Status::failed_precondition(
                    "file packet stream ended before PACKET_FIN",
                ))?;
            }
            info!("published FileSend packet export");
        };

        Ok(Response::new(Box::pin(out_stream)))
    }
}

#[derive(Default, Debug)]
pub(crate) struct UploadProvider {
    pub(crate) store: HashMap<String, Vec<u8>>,
}

impl UploadProvider {
    pub(crate) fn new() -> Self {
        Self {
            store: HashMap::new(),
        }
    }

    pub(crate) fn add(&mut self, reader: Vec<u8>) -> String {
        let id = new_id();
        let key = format!("http://buildkit-session/{}", id);

        self.store.insert(format!("/{}", id), reader);
        key
    }
}

/// Chunk size for streaming the build context to buildkit. Kept well below
/// buildkit's default 16 MiB gRPC receive cap so a single message never
/// exceeds it regardless of context size.
const UPLOAD_CHUNK_SIZE: usize = 32 * 1024;

#[tonic::async_trait]
impl Upload for UploadProvider {
    type PullStream = Pin<Box<dyn Stream<Item = Result<UploadBytesMessage, Status>> + Send>>;

    async fn pull(
        &self,
        request: Request<Streaming<UploadBytesMessage>>,
    ) -> Result<Response<Self::PullStream>, Status> {
        let key = request
            .metadata()
            .get("urlpath")
            .and_then(|key| key.to_str().ok())
            .map(String::from)
            .and_then(|str| self.store.get(&str));
        if let Some(read) = key {
            let data = read.to_owned();
            let out_stream = async_stream::try_stream! {
                for chunk in data.chunks(UPLOAD_CHUNK_SIZE) {
                    yield UploadBytesMessage { data: chunk.to_vec() };
                }
            };

            Ok(Response::new(Box::pin(out_stream)))
        } else {
            Err(Status::invalid_argument(
                "invalid 'urlpath' in uploadprovider request",
            ))
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct AuthProvider {
    auth_config_cache: HashMap<String, DockerCredentials>,
    registry_token: Option<String>,
    token_seeds: HashMap<String, Bytes>,
}

const DEFAULT_TOKEN_EXPIRATION: i64 = 60;
const DOCKER_HUB_REGISTRY_HOST: &str = "https://index.docker.io/v1/";
const DOCKER_HUB_CONFIG_FILE_KEY: &str = "registry-1.docker.io";

enum TokenExpiry {
    DEFAULT,
    EXPIRES(i64),
}

struct TokenOptions {
    realm: String,
    service: String,
    scopes: Vec<String>,
    username: String,
    secret: String,
    fetch_refresh_token: bool,
}

#[derive(Debug, Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
    issued_at: GrpcDateTime,
    scope: String,
}

impl AuthProvider {
    pub(crate) fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    pub(crate) fn set_docker_credentials(
        &mut self,
        host: &str,
        docker_credentials: DockerCredentials,
    ) {
        self.auth_config_cache
            .insert(String::from(host), docker_credentials);
    }

    fn get_auth_config(&self, mut host: &str) -> Option<DockerCredentials> {
        if host == DOCKER_HUB_REGISTRY_HOST {
            host = DOCKER_HUB_CONFIG_FILE_KEY;
        }

        self.auth_config_cache
            .get(host)
            .map(DockerCredentials::to_owned)
    }

    fn to_token_response(
        &self,
        token: &str,
        issued_at: GrpcDateTime,
        expires: TokenExpiry,
    ) -> FetchTokenResponse {
        let expires = match expires {
            TokenExpiry::DEFAULT => DEFAULT_TOKEN_EXPIRATION,
            TokenExpiry::EXPIRES(expiry) => expiry,
        };

        FetchTokenResponse {
            token: String::from(token),
            expires_in: expires,
            issued_at: grpc_timestamp(&issued_at),
        }
    }

    fn get_credentials(&self, host: &str) -> Result<CredentialsResponse, Status> {
        if let Some(ac) = self.get_auth_config(host) {
            match ac {
                DockerCredentials {
                    identitytoken: Some(identitytoken),
                    ..
                } => Ok(CredentialsResponse {
                    username: String::new(),
                    secret: identitytoken,
                }),
                DockerCredentials {
                    username: Some(username),
                    password: Some(password),
                    ..
                } => Ok(CredentialsResponse {
                    username,
                    secret: password,
                }),
                DockerCredentials { .. } => {
                    Err(Status::unknown("Invalid DockerCredentials provided"))
                }
            }
        } else {
            Ok(CredentialsResponse {
                ..Default::default()
            })
        }
    }

    fn ssl_client(
    ) -> Result<Client<hyper_rustls::HttpsConnector<HttpConnector>, BodyType>, GrpcAuthError> {
        let mut root_store = rustls::RootCertStore::empty();

        #[cfg(not(any(feature = "test_ssl", feature = "webpki")))]
        let native_certs = rustls_native_certs::load_native_certs();

        #[cfg(not(any(feature = "test_ssl", feature = "webpki")))]
        if native_certs.errors.is_empty() {
            for cert in native_certs.certs {
                root_store
                    .add(cert)
                    .map_err(|err| GrpcAuthError::RustTlsError { err })?
            }
        } else {
            return Err(GrpcAuthError::RustlsNativeCertsErrors {
                errors: native_certs.errors,
            });
        }
        #[cfg(any(feature = "test_ssl", feature = "webpki"))]
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        let config = rustls::ClientConfig::builder_with_protocol_versions(ALL_VERSIONS)
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let mut http_connector = HttpConnector::new();
        http_connector.enforce_http(false);

        let https_connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_tls_config(config)
            .https_or_http()
            .enable_http1()
            .build();

        let client_builder = Client::builder(TokioExecutor::new());
        let client = client_builder.build(https_connector);

        Ok(client)
    }

    async fn fetch_token_with_oauth(
        &self,
        opts: &TokenOptions,
    ) -> Result<OAuthTokenResponse, GrpcAuthError> {
        let mut form = vec![];
        form.push(("client_id", "bollard-client"));
        let scopes = opts.scopes.join(" ");
        if !opts.scopes.is_empty() {
            form.push(("scope", &scopes));
        }
        form.push(("service", &opts.service));
        if opts.username.is_empty() {
            form.push(("grant_type", "refresh_token"));
            form.push(("refresh_token", &opts.secret));
        } else {
            form.push(("grant_type", "password"));
            form.push(("username", &opts.username));
            form.push(("password", &opts.secret));
        }
        if opts.fetch_refresh_token {
            form.push(("access_type", "offline"));
        }

        let params = serde_urlencoded::to_string(form)?;

        let client = Self::ssl_client()?;

        let full_uri = format!("{}?{}", opts.realm, &params);
        let request_uri: hyper::Uri = full_uri.try_into()?;
        let request =
            hyper::Request::post(request_uri).body(BodyType::Left(Full::new(Bytes::new())))?;

        let response = client.request(request).await?;

        let status = response.status().as_u16();
        if !(200..400).contains(&status) {
            // return custom error
            return Err(GrpcAuthError::BadRegistryResponse {
                status_code: status,
            });
        }

        let bytes = response.into_body().collect().await.unwrap().to_bytes();

        let oauth_token = serde_json::from_slice::<OAuthTokenResponse>(&bytes)?;

        Ok(oauth_token)
    }
}

#[tonic::async_trait]
impl Auth for AuthProvider {
    async fn credentials(
        &self,
        request: Request<CredentialsRequest>,
    ) -> Result<Response<CredentialsResponse>, Status> {
        let host = request.get_ref().host.as_ref();

        Ok(Response::new(self.get_credentials(host)?))
    }

    async fn fetch_token(
        &self,
        request: Request<FetchTokenRequest>,
    ) -> Result<Response<FetchTokenResponse>, Status> {
        let FetchTokenRequest {
            client_id: _,
            host,
            realm,
            service,
            scopes,
        } = request.get_ref();

        let creds = self.get_credentials(host)?;

        // check for statically configured bearer token
        if let Some(token) = self.registry_token.as_ref() {
            Ok(Response::new(self.to_token_response(
                token,
                grpc_now(),
                TokenExpiry::DEFAULT,
            )))
        } else {
            let to = TokenOptions {
                realm: String::clone(realm),
                service: String::clone(service),
                scopes: Vec::clone(scopes),
                username: creds.username,
                secret: creds.secret,
                fetch_refresh_token: false,
            };

            match self.fetch_token_with_oauth(&to).await {
                Ok(res) => Ok(Response::new(self.to_token_response(
                    &res.access_token,
                    res.issued_at,
                    TokenExpiry::EXPIRES(res.expires_in),
                ))),
                Err(e) => Err(Status::from_error(Box::new(e))),
            }
        }
    }

    #[allow(clippy::diverging_sub_expression)]
    async fn get_token_authority(
        &self,
        _request: Request<GetTokenAuthorityRequest>,
    ) -> Result<Response<GetTokenAuthorityResponse>, Status> {
        return Err(Status::unavailable("client-side authentication disabled"));
    }

    #[allow(clippy::diverging_sub_expression)]
    async fn verify_token_authority(
        &self,
        _request: Request<VerifyTokenAuthorityRequest>,
    ) -> Result<Response<VerifyTokenAuthorityResponse>, Status> {
        return Err(Status::unavailable("client-side authentication disabled"));
    }
}

#[derive(Default, Debug)]
pub(crate) struct SecretProvider {
    pub(crate) store: HashMap<String, build::SecretSource>,
}

impl SecretProvider {
    pub(crate) fn new(store: HashMap<String, build::SecretSource>) -> Self {
        Self { store }
    }
}

#[tonic::async_trait]
impl Secrets for SecretProvider {
    async fn get_secret(
        &self,
        request: Request<GetSecretRequest>,
    ) -> Result<Response<GetSecretResponse>, Status> {
        let id: &str = request.get_ref().id.as_ref();

        match self.store.get(id) {
            Some(build::SecretSource::File(path)) if path.exists() => {
                match tokio::fs::metadata(&path).await {
                    Ok(metadata) => {
                        if metadata.len() > MAX_SECRET_SIZE {
                            return Err(Status::failed_precondition(format!(
                                "invalid secret size {}",
                                metadata.len(),
                            )));
                        }
                    }
                    Err(e) => return Err(Status::from_error(e.into())),
                }

                match tokio::fs::read(path).await {
                    Ok(contents) => Ok(Response::new(GetSecretResponse { data: contents })),
                    Err(e) => Err(Status::from_error(e.into())),
                }
            }
            Some(build::SecretSource::File(path)) => Err(Status::failed_precondition(format!(
                "path does not exist '{:?}'",
                path
            ))),
            Some(build::SecretSource::Env(v)) if env::var_os(v).is_some() => {
                trace!("Getting secret env var {}", v);
                Ok(Response::new(GetSecretResponse {
                    data: env::var_os(v).unwrap().as_encoded_bytes().to_owned(),
                }))
            }
            Some(build::SecretSource::Env(v)) => Err(Status::failed_precondition(format!(
                "env var '{}' does not exist",
                v
            ))),

            None => return Err(Status::not_found("secret missing ID")),
        }
    }
}

/// BuildKit's own id for the agent a `RUN --mount=type=ssh` instruction gets
/// when it names none — and what an empty id means in both RPCs below.
///
/// Ref: `DefaultID` in
/// <https://github.com/moby/buildkit/blob/master/session/sshforward/ssh.go>
pub(crate) const DEFAULT_SSH_AGENT_ID: &str = "default";

/// The gRPC metadata key BuildKit puts the requested agent id under when it
/// opens a `ForwardAgent` stream. Note the dots: this is `buildkit.ssh.id`,
/// not a hyphenated spelling — getting it wrong doesn't fail loudly, it
/// silently routes every named agent to `default`.
///
/// Ref: `KeySSHID` in
/// <https://github.com/moby/buildkit/blob/master/session/sshforward/ssh.go>
const SSH_ID_METADATA_KEY: &str = "buildkit.ssh.id";

/// Applies BuildKit's "an empty id means [`DEFAULT_SSH_AGENT_ID`]" rule.
///
/// Shared by both RPCs deliberately: they read the id from different places
/// (`CheckAgent` from the request body, `ForwardAgent` from stream metadata)
/// but must agree on what it means, or a build passes `check_agent` and then
/// fails to forward.
fn resolve_agent_id(id: &str) -> &str {
    if id.is_empty() {
        DEFAULT_SSH_AGENT_ID
    } else {
        id
    }
}

/// The agent id a `ForwardAgent` stream is for.
///
/// Absent metadata, a non-ASCII value, or an empty one all mean
/// [`DEFAULT_SSH_AGENT_ID`] — matching BuildKit's own provider, which
/// overrides the default only when the key is present and non-empty.
///
/// Split out from `forward_agent` so it can be tested against a hand-built
/// [`MetadataMap`](tonic::metadata::MetadataMap): the surrounding code needs a
/// live `Streaming` request, which a unit test can't construct.
fn agent_id_from_metadata(metadata: &tonic::metadata::MetadataMap) -> &str {
    resolve_agent_id(
        metadata
            .get(SSH_ID_METADATA_KEY)
            .and_then(|id| id.to_str().ok())
            .unwrap_or_default(),
    )
}

/// Where a named ssh agent's bytes are relayed to.
///
/// Registered with
/// [`ImageBuildSessionProviders::set_ssh_agent`](crate::grpc::build::ImageBuildSessionProviders::set_ssh_agent).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SshAgentSource {
    /// The agent the host's `SSH_AUTH_SOCK` points at.
    ///
    /// Resolved when the build actually asks for the agent rather than when
    /// it is registered, so constructing a build configuration never reads
    /// the environment and a missing `SSH_AUTH_SOCK` is reported as a build
    /// error rather than swallowed at registration time.
    DefaultAgentSocket,
    /// A specific Unix socket that speaks the ssh-agent protocol. It need not
    /// be a running `ssh-agent`: anything answering that protocol works, which
    /// is what lets a caller serve keys it holds itself.
    Socket(PathBuf),
}

impl SshAgentSource {
    /// The Unix socket to relay to, given the host's current `SSH_AUTH_SOCK`
    /// (`None` when unset).
    ///
    /// Takes the environment's value as an argument rather than reading it,
    /// so the fallback rule is testable without mutating process-global state
    /// from a test — [`SshProvider::socket_for`] is the single place that
    /// actually consults the environment.
    fn resolve(&self, default_agent_socket: Option<OsString>) -> Result<PathBuf, GrpcSshError> {
        match self {
            SshAgentSource::DefaultAgentSocket => default_agent_socket
                .filter(|socket| !socket.is_empty())
                .map(PathBuf::from)
                .ok_or_else(|| {
                    GrpcSshError::SshAgentSocketInit(String::from(
                        "The environment variable SSH_AUTH_SOCK is missing, and is required for the sshforwarding functionality",
                    ))
                }),
            SshAgentSource::Socket(path) => Ok(PathBuf::clone(path)),
        }
    }
}

#[derive(Debug)]
pub(crate) struct SshProvider {
    /// Agent id → where to relay it. Empty ids are resolved to
    /// [`DEFAULT_SSH_AGENT_ID`] before lookup, never stored as `""`.
    sources: HashMap<String, SshAgentSource>,
}

impl SshProvider {
    pub(crate) fn new(sources: HashMap<String, SshAgentSource>) -> Self {
        Self { sources }
    }

    /// Resolves an already-[`resolve_agent_id`]d id to the socket its bytes
    /// go to, or explains why it can't be.
    ///
    /// The one place the environment is consulted, so `check_agent` cannot
    /// accept an agent that `forward_agent` would then refuse.
    fn socket_for(&self, id: &str) -> Result<PathBuf, GrpcSshError> {
        let source = self.sources.get(id).ok_or_else(|| {
            GrpcSshError::SshAgentSocketInit(format!(
                "No ssh agent is registered under the id '{id}' requested by this build"
            ))
        })?;
        source.resolve(env::var_os("SSH_AUTH_SOCK"))
    }

    /// Connects to the agent registered under an already-[`resolve_agent_id`]d
    /// `id`.
    ///
    /// Names both the agent and the socket when it can't: with more than one
    /// agent registered, a bare "No such file or directory" doesn't say which
    /// one failed. BuildKit's own provider wraps this the same way
    /// (`failed to dial agent %s`).
    ///
    /// Split out of `forward_agent` so that message is testable at all — that
    /// method needs a live `Streaming` request, this needs only a provider.
    #[cfg(not(windows))]
    async fn connect(&self, id: &str) -> Result<tokio::net::UnixStream, GrpcSshError> {
        let socket_path = self.socket_for(id)?;
        tokio::net::UnixStream::connect(&socket_path)
            .await
            .map_err(|e| {
                GrpcSshError::SshAgentSocketInit(format!(
                    "Failed to connect to the ssh agent '{}' at {}: {}",
                    id,
                    socket_path.display(),
                    e
                ))
            })
    }
}

#[tonic::async_trait]
impl Ssh for SshProvider {
    async fn check_agent(
        &self,
        request: Request<CheckAgentRequest>,
    ) -> Result<Response<CheckAgentResponse>, Status> {
        // `CheckAgent` carries the id in the request body; `ForwardAgent`
        // carries it in stream metadata. Both go through `socket_for`, so a
        // build that passes this check can't then fail to forward.
        let id = resolve_agent_id(request.get_ref().id.as_ref());
        self.socket_for(id)
            .map_err(|e| Status::from(std::io::Error::other(e)))?;
        Ok(Response::new(CheckAgentResponse {}))
    }

    /// Server streaming response type for the ForwardAgent method.
    type ForwardAgentStream = Pin<
        Box<
            dyn Stream<
                    Item = Result<
                        bollard_buildkit_proto::moby::sshforward::v1::BytesMessage,
                        Status,
                    >,
                > + Send
                + 'static,
        >,
    >;

    #[cfg(not(windows))]
    async fn forward_agent(
        &self,
        request: Request<Streaming<bollard_buildkit_proto::moby::sshforward::v1::BytesMessage>>,
    ) -> Result<Response<Self::ForwardAgentStream>, Status> {
        // Which agent this stream is for. Already validated by `check_agent`,
        // but resolved again rather than remembered: nothing guarantees the
        // two are called in that order, or at all, and this is the call that
        // actually needs the socket.
        let id = agent_id_from_metadata(request.metadata()).to_owned();
        let sock = self
            .connect(&id)
            .await
            .map_err(|e| Status::from(std::io::Error::other(e)))?;

        let (tx, rx) = mpsc::channel::<Result<Bytes, Status>>(100);
        let rx_stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(
            |res: Result<Bytes, _>| match res {
                Ok(v) => Ok(bollard_buildkit_proto::moby::sshforward::v1::BytesMessage {
                    data: v.to_vec(),
                }),
                Err(e) => Err(Status::from_error(e.into())),
            },
        );

        let in_stream = request.into_inner();
        let mut in_framed = FramedRead::new(
            StreamReader::new(in_stream.map(|res| match res {
                Ok(bollard_buildkit_proto::moby::sshforward::v1::BytesMessage { data: bytes }) => {
                    Ok(Bytes::from(bytes))
                }
                Err(e) => Err(std::io::Error::other(e)),
            })),
            SshAgentPacketDecoder::new(),
        );

        let (sock_read, sock_write) = sock.into_split();

        let output_reader = ReaderStream::new(sock_read).map(|res| match res {
            Ok(v) => {
                Ok(bollard_buildkit_proto::moby::sshforward::v1::BytesMessage { data: v.to_vec() })
            }
            Err(e) => Err(Status::from_error(e.into())),
        });

        tokio::spawn(async move {
            if let Err(e) = sock_write.writable().await {
                tx.send(Err(Status::from(e)))
                    .await
                    .unwrap_or_else(|e| log::error!("ssh agent socket not writable: {e}"));
                panic!("ssh agent socket not writable");
            }
            while let Some(result) = in_framed.next().await {
                match result {
                    Ok(data) => {
                        if let Err(e) = sock_write.try_write(&data) {
                            tx.send(Err(Status::from(e))).await.unwrap_or_else(|e| {
                                log::error!("Failed to send error to channel: {e}")
                            });
                            break;
                        }
                    }
                    Err(err) => {
                        tx.send(Err(Status::from(std::io::Error::other(err))))
                            .await
                            .unwrap_or_else(|e| {
                                log::error!("Failed to send error to channel: {e}")
                            });
                        break;
                    }
                }
            }
            sock_write.forget();
        });

        let combined_output_stream =
            futures_util::stream::iter(vec![output_reader.right_stream(), rx_stream.left_stream()])
                .flatten_unordered(None);

        Ok(Response::new(Box::pin(combined_output_stream)))
    }

    #[cfg(windows)]
    async fn forward_agent(
        &self,
        request: Request<Streaming<bollard_buildkit_proto::moby::sshforward::v1::BytesMessage>>,
    ) -> Result<Response<Self::ForwardAgentStream>, Status> {
        unimplemented!();
    }
}

pub(crate) struct GrpcClient {
    pub(crate) client: crate::Docker,
    pub(crate) session_id: String,
}

impl Service<tonic::transport::Uri> for GrpcClient {
    type Response = GrpcTransport;
    type Error = error::GrpcError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: tonic::transport::Uri) -> Self::Future {
        // create the body
        let opt: Option<serde_json::Value> = None;
        let url = "/grpc";
        let client = self.client.clone();
        let req = client.build_request(
            url,
            Builder::new()
                .method(Method::POST)
                .header("Connection", "Upgrade")
                .header("Upgrade", "h2c")
                .header("X-Docker-Expose-Session-Uuid", &self.session_id),
            opt,
            Ok(BodyType::Left(Full::new(Bytes::new()))),
        );
        let fut = async move {
            client.process_upgraded(req).await.map(|(read, write)| {
                let output = Box::pin(read);
                let input = Box::pin(write);
                GrpcTransport {
                    read: output,
                    write: input,
                }
            })
        };

        // Return the response as an immediate future
        Box::pin(fut.map_err(From::from))
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
/// A reference to a build within a BuildKit session
/// It may be used to keep track of the progress of a build in BuildKit.
///
/// See [`bollard_buildkit_proto::moby::buildkit::v1::control_client::ControlClient::status`].
pub struct BuildRef(String);

impl From<BuildRef> for String {
    fn from(value: BuildRef) -> Self {
        value.0
    }
}

impl AsRef<str> for BuildRef {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Default for BuildRef {
    fn default() -> Self {
        Self::random()
    }
}

impl BuildRef {
    /// Generate a new, random BuildRef
    pub fn random() -> Self {
        Self(new_id())
    }
}

// Reference: https://github.com/moby/buildkit/blob/master/identity/randomid.go
pub(crate) fn new_id() -> String {
    let mut p: [u8; 17] = Default::default();
    rand::fill(&mut p);
    p[0] |= 0x80; // set high bit to avoid the need for padding
    num::BigInt::from_bytes_be(num::bigint::Sign::Plus, &p[..]).to_str_radix(36)[1..26].to_string()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::ffi::OsString;
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use bollard_buildkit_proto::fsutil::types::packet::PacketType;
    use bollard_buildkit_proto::fsutil::types::{Packet, Stat};
    use bollard_buildkit_proto::moby::filesync::packet::file_send_client::FileSendClient;
    use bollard_buildkit_proto::moby::sshforward::v1::ssh_server::Ssh;
    use bollard_buildkit_proto::moby::sshforward::v1::CheckAgentRequest;
    use tokio::{net::TcpListener, sync::mpsc};
    use tokio_stream::wrappers::TcpListenerStream;
    use tonic::metadata::MetadataMap;
    use tonic::transport::Server;
    use tonic::Request;

    use super::build::ImageBuildSessionProviders;
    use super::{
        fs, fsutil, prepare_staging_directory, publish_staging_directory, FileReceiveState,
        FileSendPacketImpl, FileSendPacketServer, FileTransferLimits, SshAgentSource, SshProvider,
        MAX_FILE_SIZE,
    };

    fn packet_stat(stat: Option<Stat>) -> Packet {
        Packet {
            r#type: PacketType::PacketStat.into(),
            stat,
            id: 0,
            data: vec![],
        }
    }

    fn packet_data(id: u32, data: &[u8]) -> Packet {
        Packet {
            r#type: PacketType::PacketData.into(),
            stat: None,
            id,
            data: data.to_vec(),
        }
    }

    fn packet_fin() -> Packet {
        Packet {
            r#type: PacketType::PacketFin.into(),
            stat: None,
            id: 0,
            data: vec![],
        }
    }

    fn stat(path: &str, mode: u32, size: i64, linkname: &str) -> Stat {
        Stat {
            path: path.to_string(),
            mode,
            uid: 0,
            gid: 0,
            size,
            mod_time: 0,
            linkname: linkname.to_string(),
            devmajor: 0,
            devminor: 0,
            xattrs: HashMap::new(),
        }
    }

    async fn start_file_send_server(
        destination: std::path::PathBuf,
    ) -> (
        std::net::SocketAddr,
        tokio::task::JoinHandle<Result<(), tonic::transport::Error>>,
    ) {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = FileSendPacketServer::new(FileSendPacketImpl::new(&destination));
        let task = tokio::spawn(async move {
            Server::builder()
                .add_service(server)
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
        });
        (address, task)
    }

    fn transfer_sibling_names(root: &Path) -> Vec<String> {
        std::fs::read_dir(root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".bollard-staging-") || name.contains(".bollard-backup-"))
            .collect()
    }

    async fn wait_for_staging_siblings(root: &Path, expected: bool) {
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if !transfer_sibling_names(root).is_empty() == expected {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("FileSend staging state became observable");
    }

    #[test]
    fn test_new_id() {
        let s = super::new_id();
        assert_eq!(s.len(), 25);
    }

    #[test]
    fn test_safe_path_rejects_unsafe_components() {
        for path in ["", ".", "..", "../escape", "/absolute", "foo/../bar"] {
            assert!(FileSendPacketImpl::validate_path(path).is_err(), "{path:?}");
        }
    }

    #[tokio::test]
    async fn test_file_receive_state_enforces_transfer_limits() {
        let root = tempfile::tempdir().unwrap();
        let mut state = FileReceiveState::with_limits(
            root.path().to_path_buf(),
            FileTransferLimits {
                max_files: Some(1),
                max_bytes: Some(4),
            },
        )
        .await
        .unwrap();

        state
            .handle_packet(packet_stat(Some(stat("first", 0o644, 4, ""))))
            .await
            .unwrap();
        let error = state
            .handle_packet(packet_stat(Some(stat("second", 0o644, 0, ""))))
            .await
            .unwrap_err();
        assert!(error.message().contains("maximum entry count"));

        let mut state = FileReceiveState::with_limits(
            root.path().to_path_buf(),
            FileTransferLimits {
                max_files: None,
                max_bytes: Some(3),
            },
        )
        .await
        .unwrap();
        let error = state
            .handle_packet(packet_stat(Some(stat("too-large", 0o644, 4, ""))))
            .await
            .unwrap_err();
        assert!(error.message().contains("maximum byte count"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_file_send_packet_grpc_replaces_destination_and_preserves_entries() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("output");
        fs::create_dir(&destination).await.unwrap();
        fs::write(destination.join("sentinel"), b"old")
            .await
            .unwrap();

        let (address, server_task) = start_file_send_server(destination.clone()).await;
        let mut client = FileSendClient::connect(format!("http://{address}"))
            .await
            .unwrap();
        let packets = vec![
            packet_stat(Some(stat(
                "subdir",
                fsutil::FileMode::Dir.bits() | 0o750,
                0,
                "",
            ))),
            packet_stat(Some(stat("message", 0o640, 5, ""))),
            packet_stat(Some(stat("empty", 0o600, 0, ""))),
            packet_stat(Some(stat(
                "link",
                fsutil::FileMode::Symlink.bits() | 0o777,
                0,
                "message",
            ))),
            packet_stat(None),
            packet_data(1, b"hello"),
            packet_data(1, b""),
            packet_data(2, b""),
            packet_fin(),
        ];
        let response = client.diff_copy(tokio_stream::iter(packets)).await.unwrap();
        let mut response_stream = response.into_inner();
        let mut requests = Vec::new();
        let mut sent_fin = false;
        while let Some(packet) = response_stream.message().await.unwrap() {
            if packet.r#type == PacketType::PacketReq as i32 {
                requests.push(packet.id);
            } else if packet.r#type == PacketType::PacketFin as i32 {
                sent_fin = true;
                assert!(!destination.join("sentinel").exists());
                assert_eq!(
                    fs::read(destination.join("message")).await.unwrap(),
                    b"hello"
                );
            }
        }
        assert_eq!(requests, vec![1, 2]);
        assert!(sent_fin);
        server_task.abort();
        let _ = server_task.await;

        assert!(!destination.join("sentinel").exists());
        assert_eq!(
            fs::read(destination.join("message")).await.unwrap(),
            b"hello"
        );
        assert!(fs::read(destination.join("empty"))
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            fs::read_link(destination.join("link")).await.unwrap(),
            Path::new("message")
        );
        assert!(transfer_sibling_names(root.path()).is_empty());
        #[cfg(unix)]
        {
            assert_eq!(
                destination.join("message").metadata().unwrap().mode() & 0o777,
                0o640
            );
            assert_eq!(
                destination.join("empty").metadata().unwrap().mode() & 0o777,
                0o600
            );
            assert_eq!(
                destination.join("subdir").metadata().unwrap().mode() & 0o777,
                0o750
            );
        }
    }

    #[tokio::test]
    async fn test_file_send_packet_grpc_accepts_sender_eof_after_publish() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("output");

        let (address, server_task) = start_file_send_server(destination.clone()).await;
        let mut client = FileSendClient::connect(format!("http://{address}"))
            .await
            .unwrap();
        let packets = vec![
            packet_stat(Some(stat("message", 0o640, 5, ""))),
            packet_stat(None),
            packet_data(0, b"hello"),
            packet_data(0, b""),
        ];
        let response = client.diff_copy(tokio_stream::iter(packets)).await.unwrap();
        let mut response_stream = response.into_inner();
        let mut sent_fin = false;
        while let Some(packet) = response_stream.message().await.unwrap() {
            if packet.r#type == PacketType::PacketFin as i32 {
                sent_fin = true;
                assert_eq!(
                    fs::read(destination.join("message")).await.unwrap(),
                    b"hello"
                );
            }
        }

        assert!(sent_fin);
        assert!(transfer_sibling_names(root.path()).is_empty());
        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_file_send_packet_grpc_cleans_staging_after_rejected_packet() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("output");
        fs::create_dir(&destination).await.unwrap();
        fs::write(destination.join("sentinel"), b"old")
            .await
            .unwrap();

        let (address, server_task) = start_file_send_server(destination.clone()).await;
        let mut client = FileSendClient::connect(format!("http://{address}"))
            .await
            .unwrap();
        let response = client
            .diff_copy(tokio_stream::iter(vec![packet_stat(Some(stat(
                "../escape",
                0o600,
                0,
                "",
            )))]))
            .await
            .unwrap();
        let mut response_stream = response.into_inner();
        let error = response_stream.message().await.unwrap_err();
        assert_eq!(error.code(), tonic::Code::InvalidArgument);
        server_task.abort();
        let _ = server_task.await;

        assert_eq!(
            fs::read(destination.join("sentinel")).await.unwrap(),
            b"old"
        );
        assert!(transfer_sibling_names(root.path()).is_empty());
    }

    #[tokio::test]
    async fn test_file_send_packet_grpc_cleans_staging_after_stream_cancellation() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("output");
        let (address, server_task) = start_file_send_server(destination).await;
        let mut client = FileSendClient::connect(format!("http://{address}"))
            .await
            .unwrap();
        let (sender, receiver) = mpsc::channel(1);
        let response = client
            .diff_copy(tokio_stream::wrappers::ReceiverStream::new(receiver))
            .await
            .unwrap();
        let mut response_stream = response.into_inner();
        let response_task =
            tokio::spawn(async move { while response_stream.message().await.is_ok() {} });

        sender
            .send(packet_stat(Some(stat("partial", 0o600, 5, ""))))
            .await
            .unwrap();
        wait_for_staging_siblings(root.path(), true).await;
        response_task.abort();
        let _ = response_task.await;
        drop(sender);
        wait_for_staging_siblings(root.path(), false).await;

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_file_send_packet_grpc_cleans_partial_staging_after_stream_cancellation() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("output");
        let (address, server_task) = start_file_send_server(destination).await;
        let mut client = FileSendClient::connect(format!("http://{address}"))
            .await
            .unwrap();
        let (sender, receiver) = mpsc::channel(1);
        let response = client
            .diff_copy(tokio_stream::wrappers::ReceiverStream::new(receiver))
            .await
            .unwrap();
        let mut response_stream = response.into_inner();
        let response_task =
            tokio::spawn(async move { while response_stream.message().await.is_ok() {} });

        sender
            .send(packet_stat(Some(stat("partial", 0o600, 5, ""))))
            .await
            .unwrap();
        sender.send(packet_data(0, b"hi")).await.unwrap();
        wait_for_staging_siblings(root.path(), true).await;
        response_task.abort();
        let _ = response_task.await;
        drop(sender);
        wait_for_staging_siblings(root.path(), false).await;

        server_task.abort();
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_file_receive_state_rejects_duplicate_paths() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = FileReceiveState::new(dir.path().to_path_buf())
            .await
            .unwrap();

        state
            .handle_packet(packet_stat(Some(stat("duplicate", 0o644, 0, ""))))
            .await
            .unwrap();
        let error = state
            .handle_packet(packet_stat(Some(stat("duplicate", 0o644, 0, ""))))
            .await
            .unwrap_err();

        assert_eq!(error.code(), tonic::Code::AlreadyExists);
    }

    #[tokio::test]
    async fn test_file_receive_state_rejects_oversized_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = FileReceiveState::new(dir.path().to_path_buf())
            .await
            .unwrap();

        let error = state
            .handle_packet(packet_stat(Some(stat(
                "oversized",
                0o644,
                (MAX_FILE_SIZE + 1) as i64,
                "",
            ))))
            .await
            .unwrap_err();

        assert_eq!(error.code(), tonic::Code::ResourceExhausted);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_file_receive_state_uses_restrictive_initial_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = FileReceiveState::new(dir.path().to_path_buf())
            .await
            .unwrap();

        state
            .handle_packet(packet_stat(Some(stat("private", 0o644, 0, ""))))
            .await
            .unwrap();

        assert_eq!(
            std::fs::metadata(dir.path().join("private"))
                .unwrap()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[tokio::test]
    async fn test_publish_staging_directory_replaces_destination() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("output");
        fs::create_dir(&destination).await.unwrap();
        fs::write(destination.join("old"), b"old").await.unwrap();

        let staging = prepare_staging_directory(&destination).await.unwrap();
        fs::write(staging.join("new"), b"new").await.unwrap();
        publish_staging_directory(&staging, &destination)
            .await
            .unwrap();

        assert!(!destination.join("old").exists());
        assert_eq!(fs::read(destination.join("new")).await.unwrap(), b"new");
    }

    #[tokio::test]
    async fn test_file_receive_state_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = FileReceiveState::new(dir.path().to_path_buf())
            .await
            .unwrap();

        let request = state
            .handle_packet(packet_stat(Some(stat("hello", 0o644, 5, ""))))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(request.r#type, PacketType::PacketReq as i32);
        assert_eq!(request.id, 0);
        assert!(state
            .handle_packet(packet_stat(None))
            .await
            .unwrap()
            .is_none());
        assert!(state
            .handle_packet(packet_data(0, b"world"))
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            state
                .handle_packet(packet_data(0, b""))
                .await
                .unwrap()
                .unwrap()
                .r#type,
            PacketType::PacketFin as i32
        );

        let path = dir.path().join("hello");
        assert_eq!(std::fs::read(&path).unwrap(), b"world");
        #[cfg(unix)]
        assert_eq!(path.metadata().unwrap().mode() & 0o777, 0o644);
    }

    #[tokio::test]
    async fn test_file_receive_state_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = FileReceiveState::new(dir.path().to_path_buf())
            .await
            .unwrap();

        assert_eq!(
            state
                .handle_packet(packet_stat(Some(stat("empty", 0o640, 0, ""))))
                .await
                .unwrap()
                .unwrap()
                .r#type,
            PacketType::PacketReq as i32
        );
        state.handle_packet(packet_stat(None)).await.unwrap();
        assert_eq!(
            state
                .handle_packet(packet_data(0, b""))
                .await
                .unwrap()
                .unwrap()
                .r#type,
            PacketType::PacketFin as i32
        );

        let path = dir.path().join("empty");
        assert!(std::fs::read(&path).unwrap().is_empty());
        #[cfg(unix)]
        assert_eq!(path.metadata().unwrap().mode() & 0o777, 0o640);
    }

    #[tokio::test]
    async fn test_file_receive_state_creates_nested_directories() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = FileReceiveState::new(dir.path().to_path_buf())
            .await
            .unwrap();

        state
            .handle_packet(packet_stat(Some(stat("a/b/file", 0o600, 4, ""))))
            .await
            .unwrap();
        state.handle_packet(packet_stat(None)).await.unwrap();
        state.handle_packet(packet_data(0, b"data")).await.unwrap();
        state.handle_packet(packet_data(0, b"")).await.unwrap();

        assert_eq!(std::fs::read(dir.path().join("a/b/file")).unwrap(), b"data");
    }

    #[tokio::test]
    async fn test_file_receive_state_directory() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = FileReceiveState::new(dir.path().to_path_buf())
            .await
            .unwrap();

        state
            .handle_packet(packet_stat(Some(stat(
                "app",
                fsutil::FileMode::Dir.bits() | 0o755,
                0,
                "",
            ))))
            .await
            .unwrap();
        assert_eq!(
            state
                .handle_packet(packet_stat(None))
                .await
                .unwrap()
                .unwrap()
                .r#type,
            PacketType::PacketFin as i32
        );
        state.finalize().await.unwrap();

        let path = dir.path().join("app");
        assert!(path.is_dir());
        #[cfg(unix)]
        assert_eq!(path.metadata().unwrap().mode() & 0o777, 0o755);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_file_receive_state_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = FileReceiveState::new(dir.path().to_path_buf())
            .await
            .unwrap();

        state
            .handle_packet(packet_stat(Some(stat(
                "link",
                fsutil::FileMode::Symlink.bits() | 0o777,
                0,
                "/target",
            ))))
            .await
            .unwrap();
        assert_eq!(
            state
                .handle_packet(packet_stat(None))
                .await
                .unwrap()
                .unwrap()
                .r#type,
            PacketType::PacketFin as i32
        );

        assert_eq!(
            std::fs::read_link(dir.path().join("link")).unwrap(),
            Path::new("/target")
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_file_receive_state_rejects_intermediate_symlink() {
        let dir = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("/tmp", dir.path().join("link")).unwrap();
        let mut state = FileReceiveState::new(dir.path().to_path_buf())
            .await
            .unwrap();

        let error = state
            .handle_packet(packet_stat(Some(stat("link/file", 0o644, 1, ""))))
            .await
            .unwrap_err();
        assert_eq!(error.code(), tonic::Code::InvalidArgument);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_file_receive_state_rejects_final_symlink() {
        let dir = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("/tmp", dir.path().join("file")).unwrap();
        let mut state = FileReceiveState::new(dir.path().to_path_buf())
            .await
            .unwrap();

        let error = state
            .handle_packet(packet_stat(Some(stat("file", 0o644, 1, ""))))
            .await
            .unwrap_err();
        assert_eq!(error.code(), tonic::Code::AlreadyExists);
    }

    #[tokio::test]
    async fn test_file_receive_state_rejects_unsupported_file_type() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = FileReceiveState::new(dir.path().to_path_buf())
            .await
            .unwrap();

        let error = state
            .handle_packet(packet_stat(Some(stat(
                "pipe",
                fsutil::FileMode::NamedPipe.bits(),
                0,
                "",
            ))))
            .await
            .unwrap_err();
        assert_eq!(error.code(), tonic::Code::Unimplemented);
    }

    #[tokio::test]
    async fn test_file_receive_state_rejects_truncated_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = FileReceiveState::new(dir.path().to_path_buf())
            .await
            .unwrap();

        state
            .handle_packet(packet_stat(Some(stat("hello", 0o644, 5, ""))))
            .await
            .unwrap();
        state.handle_packet(packet_stat(None)).await.unwrap();
        state.handle_packet(packet_data(0, b"hi")).await.unwrap();

        let error = state.handle_packet(packet_data(0, b"")).await.unwrap_err();
        assert!(error.message().contains("ended after 2 of 5 bytes"));
    }

    #[tokio::test]
    async fn test_file_receive_state_rejects_excess_file_data() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = FileReceiveState::new(dir.path().to_path_buf())
            .await
            .unwrap();

        state
            .handle_packet(packet_stat(Some(stat("hello", 0o644, 3, ""))))
            .await
            .unwrap();
        state.handle_packet(packet_stat(None)).await.unwrap();

        let error = state
            .handle_packet(packet_data(0, b"more"))
            .await
            .unwrap_err();
        assert_eq!(error.code(), tonic::Code::ResourceExhausted);
        assert!(std::fs::read(dir.path().join("hello")).unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_file_receive_state_rejects_invalid_packet_order() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = FileReceiveState::new(dir.path().to_path_buf())
            .await
            .unwrap();

        state.handle_packet(packet_stat(None)).await.unwrap();
        assert!(state.handle_packet(packet_stat(None)).await.is_err());
        assert!(state
            .handle_packet(packet_stat(Some(stat("late", 0o644, 0, ""))))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_file_receive_state_rejects_early_fin_and_eof() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = FileReceiveState::new(dir.path().to_path_buf())
            .await
            .unwrap();

        state
            .handle_packet(packet_stat(Some(stat("hello", 0o644, 1, ""))))
            .await
            .unwrap();
        state.handle_packet(packet_stat(None)).await.unwrap();
        assert!(state.handle_packet(packet_fin()).await.is_err());
        assert!(state.finish_stream().is_err());
    }

    #[tokio::test]
    async fn test_file_receive_state_rejects_packets_after_fin() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = FileReceiveState::new(dir.path().to_path_buf())
            .await
            .unwrap();

        state.handle_packet(packet_stat(None)).await.unwrap();
        state.handle_packet(packet_fin()).await.unwrap();
        assert!(state.handle_packet(packet_stat(None)).await.is_err());
        assert!(state.finish_stream().is_ok());
    }

    #[tokio::test]
    async fn test_file_receive_state_rejects_unknown_packets_and_ids() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = FileReceiveState::new(dir.path().to_path_buf())
            .await
            .unwrap();

        let error = state
            .handle_packet(Packet {
                r#type: 99,
                stat: None,
                id: 0,
                data: vec![],
            })
            .await
            .unwrap_err();
        assert_eq!(error.code(), tonic::Code::InvalidArgument);
        assert!(state.handle_packet(packet_data(99, b"data")).await.is_err());
    }

    #[tokio::test]
    async fn test_file_receive_state_rejects_negative_size_and_id_exhaustion() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = FileReceiveState::new(dir.path().to_path_buf())
            .await
            .unwrap();

        let error = state
            .handle_packet(packet_stat(Some(stat("negative", 0o644, -1, ""))))
            .await
            .unwrap_err();
        assert!(error.message().contains("negative size"));

        state.next_stat_id = u32::MAX;
        let error = state
            .handle_packet(packet_stat(Some(stat("overflow", 0o644, 0, ""))))
            .await
            .unwrap_err();
        assert_eq!(error.code(), tonic::Code::ResourceExhausted);
    }

    /// BuildKit puts the requested agent id in gRPC metadata under this exact
    /// key. Pinned as a literal because getting it wrong is silent: every
    /// named agent would fall back to `default` and forward to the wrong
    /// socket, with no error anywhere. Taken from `KeySSHID` in
    /// <https://github.com/moby/buildkit/blob/master/session/sshforward/ssh.go>
    /// — note the dots, not hyphens.
    #[test]
    fn ssh_id_metadata_key_matches_buildkits_own() {
        assert_eq!(super::SSH_ID_METADATA_KEY, "buildkit.ssh.id");
        assert_eq!(super::DEFAULT_SSH_AGENT_ID, "default");
    }

    #[test]
    fn an_empty_agent_id_means_default() {
        assert_eq!(super::resolve_agent_id(""), "default");
        assert_eq!(super::resolve_agent_id("deploy"), "deploy");
    }

    fn metadata_with_ssh_id(value: &str) -> MetadataMap {
        let mut metadata = MetadataMap::new();
        metadata.insert(super::SSH_ID_METADATA_KEY, value.parse().unwrap());
        metadata
    }

    #[test]
    fn a_forward_agent_stream_names_its_agent_in_metadata() {
        assert_eq!(
            super::agent_id_from_metadata(&metadata_with_ssh_id("deploy")),
            "deploy"
        );
    }

    /// The two ways BuildKit can decline to name an agent. Both mean
    /// `default`, and neither is an error — a Dockerfile's plain
    /// `RUN --mount=type=ssh` produces exactly this.
    #[test]
    fn a_forward_agent_stream_with_no_usable_id_falls_back_to_default() {
        assert_eq!(
            super::agent_id_from_metadata(&MetadataMap::new()),
            "default",
            "absent key"
        );
        assert_eq!(
            super::agent_id_from_metadata(&metadata_with_ssh_id("")),
            "default",
            "present but empty"
        );
    }

    #[test]
    fn a_socket_source_resolves_to_its_own_path() {
        let source = SshAgentSource::Socket(PathBuf::from("/tmp/deploy.sock"));

        assert_eq!(
            source.resolve(Some(OsString::from("/ignored"))).unwrap(),
            PathBuf::from("/tmp/deploy.sock"),
            "an explicit socket must not be overridden by SSH_AUTH_SOCK"
        );
    }

    #[test]
    fn the_default_source_resolves_to_ssh_auth_sock() {
        assert_eq!(
            SshAgentSource::DefaultAgentSocket
                .resolve(Some(OsString::from("/run/agent.sock")))
                .unwrap(),
            PathBuf::from("/run/agent.sock")
        );
    }

    /// Reported rather than panicked. `forward_agent` used to
    /// `.expect("missing SSH_AUTH_SOCK")` here, which took the whole process
    /// down from inside a library.
    #[test]
    fn the_default_source_reports_a_missing_ssh_auth_sock() {
        for absent in [None, Some(OsString::new())] {
            let error = SshAgentSource::DefaultAgentSocket
                .resolve(absent.clone())
                .unwrap_err();

            assert!(
                error.to_string().contains("SSH_AUTH_SOCK"),
                "{absent:?} should name the missing variable, got: {error}"
            );
        }
    }

    fn provider_with(id: &str, path: &str) -> SshProvider {
        SshProvider::new(HashMap::from([(
            String::from(id),
            SshAgentSource::Socket(PathBuf::from(path)),
        )]))
    }

    #[tokio::test]
    async fn check_agent_accepts_a_registered_named_agent() {
        let provider = provider_with("deploy", "/tmp/deploy.sock");

        provider
            .check_agent(Request::new(CheckAgentRequest {
                id: String::from("deploy"),
            }))
            .await
            .expect("a registered id must be accepted");
    }

    /// BuildKit sends an empty id for a `RUN --mount=type=ssh` that names
    /// none, so this is the path `enable_ssh(true)` alone has to serve.
    #[tokio::test]
    async fn check_agent_maps_an_empty_id_onto_the_default_agent() {
        let provider = provider_with("default", "/tmp/default.sock");

        provider
            .check_agent(Request::new(CheckAgentRequest { id: String::new() }))
            .await
            .expect("an empty id must resolve to the default agent");
    }

    #[tokio::test]
    async fn check_agent_rejects_an_agent_that_was_never_registered() {
        let provider = provider_with("deploy", "/tmp/deploy.sock");

        let status = provider
            .check_agent(Request::new(CheckAgentRequest {
                id: String::from("typo"),
            }))
            .await
            .expect_err("an unregistered id must be refused");

        assert!(
            status.message().contains("typo"),
            "the error should name the id the build asked for, got: {}",
            status.message()
        );
    }

    /// A failed dial has to say *which* agent failed, or a build with several
    /// registered leaves you guessing.
    ///
    /// The id and the socket path share no substring on purpose: with the id
    /// also appearing in the path, `contains("deploy")` passes on the path
    /// alone and the assertion silently stops checking the thing it names.
    /// (Verified — the first version of this test used
    /// `/…/deploy.sock` and passed with the id dropped from the message.)
    #[cfg(not(windows))]
    #[tokio::test]
    async fn connecting_to_a_missing_socket_names_the_agent_and_the_path() {
        const SOCKET: &str = "/nonexistent/bollard-test/agent.sock";
        let provider = provider_with("deploy", SOCKET);

        let error = provider
            .connect("deploy")
            .await
            .expect_err("a socket that isn't there can't be connected to");
        let message = error.to_string();

        assert!(
            message.contains("deploy"),
            "should name the agent, got: {message}"
        );
        assert!(
            message.contains(SOCKET),
            "should name the socket, got: {message}"
        );
    }

    /// `enable_ssh(true)` is sugar, so it has to land on exactly the state the
    /// general call produces — not merely an equivalent-looking one. Asserted
    /// rather than assumed because the two were separate fields before, and
    /// keeping two spellings of one setting in step by hand is what this
    /// consolidation exists to avoid.
    #[test]
    fn enable_ssh_is_exactly_the_default_named_agent() {
        assert_eq!(
            ImageBuildSessionProviders::default().enable_ssh(true),
            ImageBuildSessionProviders::default()
                .set_ssh_agent("default", &SshAgentSource::DefaultAgentSocket)
        );
    }

    /// Disabling the implicit agent must not disturb explicitly named ones —
    /// they were never what the flag referred to.
    #[test]
    fn disabling_ssh_leaves_named_agents_registered() {
        let providers = ImageBuildSessionProviders::default()
            .set_ssh_agent(
                "deploy",
                &SshAgentSource::Socket(PathBuf::from("/tmp/d.sock")),
            )
            .enable_ssh(true)
            .enable_ssh(false);

        assert_eq!(
            providers,
            ImageBuildSessionProviders::default().set_ssh_agent(
                "deploy",
                &SshAgentSource::Socket(PathBuf::from("/tmp/d.sock"))
            )
        );
        assert!(
            !providers.is_empty(),
            "a named agent still needs a session to serve it"
        );
    }

    #[test]
    fn providers_with_no_secrets_and_no_agents_are_empty() {
        assert!(ImageBuildSessionProviders::default().is_empty());
        assert!(ImageBuildSessionProviders::default()
            .enable_ssh(true)
            .enable_ssh(false)
            .is_empty());
    }
}
