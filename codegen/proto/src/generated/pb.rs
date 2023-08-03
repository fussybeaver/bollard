/// Op represents a vertex of the LLB DAG.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Op {
    /// inputs is a set of input edges.
    #[prost(message, repeated, tag = "1")]
    pub inputs: ::prost::alloc::vec::Vec<Input>,
    #[prost(message, optional, tag = "10")]
    pub platform: ::core::option::Option<Platform>,
    #[prost(message, optional, tag = "11")]
    pub constraints: ::core::option::Option<WorkerConstraints>,
    #[prost(oneof = "op::Op", tags = "2, 3, 4, 5, 6, 7")]
    pub op: ::core::option::Option<op::Op>,
}
/// Nested message and enum types in `Op`.
pub mod op {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Op {
        #[prost(message, tag = "2")]
        Exec(super::ExecOp),
        #[prost(message, tag = "3")]
        Source(super::SourceOp),
        #[prost(message, tag = "4")]
        File(super::FileOp),
        #[prost(message, tag = "5")]
        Build(super::BuildOp),
        #[prost(message, tag = "6")]
        Merge(super::MergeOp),
        #[prost(message, tag = "7")]
        Diff(super::DiffOp),
    }
}
/// Platform is github.com/opencontainers/image-spec/specs-go/v1.Platform
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Platform {
    #[prost(string, tag = "1")]
    pub architecture: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub os: ::prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub variant: ::prost::alloc::string::String,
    /// unused
    #[prost(string, tag = "4")]
    pub os_version: ::prost::alloc::string::String,
    /// unused
    #[prost(string, repeated, tag = "5")]
    pub os_features: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
/// Input represents an input edge for an Op.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Input {
    /// digest of the marshaled input Op
    #[prost(string, tag = "1")]
    pub digest: ::prost::alloc::string::String,
    /// output index of the input Op
    #[prost(int64, tag = "2")]
    pub index: i64,
}
/// ExecOp executes a command in a container.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ExecOp {
    #[prost(message, optional, tag = "1")]
    pub meta: ::core::option::Option<Meta>,
    #[prost(message, repeated, tag = "2")]
    pub mounts: ::prost::alloc::vec::Vec<Mount>,
    #[prost(enumeration = "NetMode", tag = "3")]
    pub network: i32,
    #[prost(enumeration = "SecurityMode", tag = "4")]
    pub security: i32,
    #[prost(message, repeated, tag = "5")]
    pub secretenv: ::prost::alloc::vec::Vec<SecretEnv>,
}
/// Meta is a set of arguments for ExecOp.
/// Meta is unrelated to LLB metadata.
/// FIXME: rename (ExecContext? ExecArgs?)
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Meta {
    #[prost(string, repeated, tag = "1")]
    pub args: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(string, repeated, tag = "2")]
    pub env: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(string, tag = "3")]
    pub cwd: ::prost::alloc::string::String,
    #[prost(string, tag = "4")]
    pub user: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "5")]
    pub proxy_env: ::core::option::Option<ProxyEnv>,
    #[prost(message, repeated, tag = "6")]
    pub extra_hosts: ::prost::alloc::vec::Vec<HostIp>,
    #[prost(string, tag = "7")]
    pub hostname: ::prost::alloc::string::String,
    #[prost(message, repeated, tag = "9")]
    pub ulimit: ::prost::alloc::vec::Vec<Ulimit>,
    #[prost(string, tag = "10")]
    pub cgroup_parent: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HostIp {
    #[prost(string, tag = "1")]
    pub host: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub ip: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Ulimit {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(int64, tag = "2")]
    pub soft: i64,
    #[prost(int64, tag = "3")]
    pub hard: i64,
}
/// SecretEnv is an environment variable that is backed by a secret.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SecretEnv {
    #[prost(string, tag = "1")]
    pub id: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
    #[prost(bool, tag = "3")]
    pub optional: bool,
}
/// Mount specifies how to mount an input Op as a filesystem.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Mount {
    #[prost(int64, tag = "1")]
    pub input: i64,
    #[prost(string, tag = "2")]
    pub selector: ::prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub dest: ::prost::alloc::string::String,
    #[prost(int64, tag = "4")]
    pub output: i64,
    #[prost(bool, tag = "5")]
    pub readonly: bool,
    #[prost(enumeration = "MountType", tag = "6")]
    pub mount_type: i32,
    #[prost(message, optional, tag = "19")]
    pub tmpfs_opt: ::core::option::Option<TmpfsOpt>,
    #[prost(message, optional, tag = "20")]
    pub cache_opt: ::core::option::Option<CacheOpt>,
    #[prost(message, optional, tag = "21")]
    pub secret_opt: ::core::option::Option<SecretOpt>,
    #[prost(message, optional, tag = "22")]
    pub ssh_opt: ::core::option::Option<SshOpt>,
    #[prost(string, tag = "23")]
    pub result_id: ::prost::alloc::string::String,
}
/// TmpfsOpt defines options describing tpmfs mounts
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TmpfsOpt {
    /// Specify an upper limit on the size of the filesystem.
    #[prost(int64, tag = "1")]
    pub size: i64,
}
/// CacheOpt defines options specific to cache mounts
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CacheOpt {
    /// ID is an optional namespace for the mount
    #[prost(string, tag = "1")]
    pub id: ::prost::alloc::string::String,
    /// Sharing is the sharing mode for the mount
    #[prost(enumeration = "CacheSharingOpt", tag = "2")]
    pub sharing: i32,
}
/// SecretOpt defines options describing secret mounts
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SecretOpt {
    /// ID of secret. Used for quering the value.
    #[prost(string, tag = "1")]
    pub id: ::prost::alloc::string::String,
    /// UID of secret file
    #[prost(uint32, tag = "2")]
    pub uid: u32,
    /// GID of secret file
    #[prost(uint32, tag = "3")]
    pub gid: u32,
    /// Mode is the filesystem mode of secret file
    #[prost(uint32, tag = "4")]
    pub mode: u32,
    /// Optional defines if secret value is required. Error is produced
    /// if value is not found and optional is false.
    #[prost(bool, tag = "5")]
    pub optional: bool,
}
/// SSHOpt defines options describing ssh mounts
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SshOpt {
    /// ID of exposed ssh rule. Used for quering the value.
    #[prost(string, tag = "1")]
    pub id: ::prost::alloc::string::String,
    /// UID of agent socket
    #[prost(uint32, tag = "2")]
    pub uid: u32,
    /// GID of agent socket
    #[prost(uint32, tag = "3")]
    pub gid: u32,
    /// Mode is the filesystem mode of agent socket
    #[prost(uint32, tag = "4")]
    pub mode: u32,
    /// Optional defines if ssh socket is required. Error is produced
    /// if client does not expose ssh.
    #[prost(bool, tag = "5")]
    pub optional: bool,
}
/// SourceOp specifies a source such as build contexts and images.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SourceOp {
    /// TODO: use source type or any type instead of URL protocol.
    /// identifier e.g. local://, docker-image://, git://, <https://...>
    #[prost(string, tag = "1")]
    pub identifier: ::prost::alloc::string::String,
    /// attrs are defined in attr.go
    #[prost(map = "string, string", tag = "2")]
    pub attrs: ::std::collections::HashMap<
        ::prost::alloc::string::String,
        ::prost::alloc::string::String,
    >,
}
/// BuildOp is used for nested build invocation.
/// BuildOp is experimental and can break without backwards compatibility
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BuildOp {
    #[prost(int64, tag = "1")]
    pub builder: i64,
    #[prost(map = "string, message", tag = "2")]
    pub inputs: ::std::collections::HashMap<::prost::alloc::string::String, BuildInput>,
    #[prost(message, optional, tag = "3")]
    pub def: ::core::option::Option<Definition>,
    /// outputs
    #[prost(map = "string, string", tag = "4")]
    pub attrs: ::std::collections::HashMap<
        ::prost::alloc::string::String,
        ::prost::alloc::string::String,
    >,
}
/// BuildInput is used for BuildOp.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BuildInput {
    #[prost(int64, tag = "1")]
    pub input: i64,
}
/// OpMetadata is a per-vertex metadata entry, which can be defined for arbitrary Op vertex and overridable on the run time.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OpMetadata {
    /// ignore_cache specifies to ignore the cache for this Op.
    #[prost(bool, tag = "1")]
    pub ignore_cache: bool,
    /// Description can be used for keeping any text fields that builder doesn't parse
    #[prost(map = "string, string", tag = "2")]
    pub description: ::std::collections::HashMap<
        ::prost::alloc::string::String,
        ::prost::alloc::string::String,
    >,
    /// index 3 reserved for WorkerConstraint in previous versions
    /// WorkerConstraint worker_constraint = 3;
    #[prost(message, optional, tag = "4")]
    pub export_cache: ::core::option::Option<ExportCache>,
    #[prost(map = "string, bool", tag = "5")]
    pub caps: ::std::collections::HashMap<::prost::alloc::string::String, bool>,
    #[prost(message, optional, tag = "6")]
    pub progress_group: ::core::option::Option<ProgressGroup>,
}
/// Source is a source mapping description for a file
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Source {
    #[prost(map = "string, message", tag = "1")]
    pub locations: ::std::collections::HashMap<
        ::prost::alloc::string::String,
        Locations,
    >,
    #[prost(message, repeated, tag = "2")]
    pub infos: ::prost::alloc::vec::Vec<SourceInfo>,
}
/// Locations is a list of ranges with a index to its source map.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Locations {
    #[prost(message, repeated, tag = "1")]
    pub locations: ::prost::alloc::vec::Vec<Location>,
}
/// Source info contains the shared metadata of a source mapping
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SourceInfo {
    #[prost(string, tag = "1")]
    pub filename: ::prost::alloc::string::String,
    #[prost(bytes = "vec", tag = "2")]
    pub data: ::prost::alloc::vec::Vec<u8>,
    #[prost(message, optional, tag = "3")]
    pub definition: ::core::option::Option<Definition>,
}
/// Location defines list of areas in to source file
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Location {
    #[prost(int32, tag = "1")]
    pub source_index: i32,
    #[prost(message, repeated, tag = "2")]
    pub ranges: ::prost::alloc::vec::Vec<Range>,
}
/// Range is an area in the source file
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Range {
    #[prost(message, optional, tag = "1")]
    pub start: ::core::option::Option<Position>,
    #[prost(message, optional, tag = "2")]
    pub end: ::core::option::Option<Position>,
}
/// Position is single location in a source file
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Position {
    #[prost(int32, tag = "1")]
    pub line: i32,
    #[prost(int32, tag = "2")]
    pub character: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ExportCache {
    #[prost(bool, tag = "1")]
    pub value: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ProgressGroup {
    #[prost(string, tag = "1")]
    pub id: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
    #[prost(bool, tag = "3")]
    pub weak: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ProxyEnv {
    #[prost(string, tag = "1")]
    pub http_proxy: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub https_proxy: ::prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub ftp_proxy: ::prost::alloc::string::String,
    #[prost(string, tag = "4")]
    pub no_proxy: ::prost::alloc::string::String,
    #[prost(string, tag = "5")]
    pub all_proxy: ::prost::alloc::string::String,
}
/// WorkerConstraints defines conditions for the worker
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct WorkerConstraints {
    /// containerd-style filter
    #[prost(string, repeated, tag = "1")]
    pub filter: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
/// Definition is the LLB definition structure with per-vertex metadata entries
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Definition {
    /// def is a list of marshaled Op messages
    #[prost(bytes = "vec", repeated, tag = "1")]
    pub def: ::prost::alloc::vec::Vec<::prost::alloc::vec::Vec<u8>>,
    /// metadata contains metadata for the each of the Op messages.
    /// A key must be an LLB op digest string. Currently, empty string is not expected as a key, but it may change in the future.
    #[prost(map = "string, message", tag = "2")]
    pub metadata: ::std::collections::HashMap<
        ::prost::alloc::string::String,
        OpMetadata,
    >,
    /// Source contains the source mapping information for the vertexes in the definition
    #[prost(message, optional, tag = "3")]
    pub source: ::core::option::Option<Source>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FileOp {
    #[prost(message, repeated, tag = "2")]
    pub actions: ::prost::alloc::vec::Vec<FileAction>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FileAction {
    /// could be real input or target (target index + max input index)
    #[prost(int64, tag = "1")]
    pub input: i64,
    /// --//--
    #[prost(int64, tag = "2")]
    pub secondary_input: i64,
    #[prost(int64, tag = "3")]
    pub output: i64,
    #[prost(oneof = "file_action::Action", tags = "4, 5, 6, 7")]
    pub action: ::core::option::Option<file_action::Action>,
}
/// Nested message and enum types in `FileAction`.
pub mod file_action {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Action {
        /// FileActionCopy copies files from secondaryInput on top of input
        #[prost(message, tag = "4")]
        Copy(super::FileActionCopy),
        /// FileActionMkFile creates a new file
        #[prost(message, tag = "5")]
        Mkfile(super::FileActionMkFile),
        /// FileActionMkDir creates a new directory
        #[prost(message, tag = "6")]
        Mkdir(super::FileActionMkDir),
        /// FileActionRm removes a file
        #[prost(message, tag = "7")]
        Rm(super::FileActionRm),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FileActionCopy {
    /// src is the source path
    #[prost(string, tag = "1")]
    pub src: ::prost::alloc::string::String,
    /// dest path
    #[prost(string, tag = "2")]
    pub dest: ::prost::alloc::string::String,
    /// optional owner override
    #[prost(message, optional, tag = "3")]
    pub owner: ::core::option::Option<ChownOpt>,
    /// optional permission bits override
    #[prost(int32, tag = "4")]
    pub mode: i32,
    /// followSymlink resolves symlinks in src
    #[prost(bool, tag = "5")]
    pub follow_symlink: bool,
    /// dirCopyContents only copies contents if src is a directory
    #[prost(bool, tag = "6")]
    pub dir_copy_contents: bool,
    /// attemptUnpackDockerCompatibility detects if src is an archive to unpack it instead
    #[prost(bool, tag = "7")]
    pub attempt_unpack_docker_compatibility: bool,
    /// createDestPath creates dest path directories if needed
    #[prost(bool, tag = "8")]
    pub create_dest_path: bool,
    /// allowWildcard allows filepath.Match wildcards in src path
    #[prost(bool, tag = "9")]
    pub allow_wildcard: bool,
    /// allowEmptyWildcard doesn't fail the whole copy if wildcard doesn't resolve to files
    #[prost(bool, tag = "10")]
    pub allow_empty_wildcard: bool,
    /// optional created time override
    #[prost(int64, tag = "11")]
    pub timestamp: i64,
    /// include only files/dirs matching at least one of these patterns
    #[prost(string, repeated, tag = "12")]
    pub include_patterns: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    /// exclude files/dir matching any of these patterns (even if they match an include pattern)
    #[prost(string, repeated, tag = "13")]
    pub exclude_patterns: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FileActionMkFile {
    /// path for the new file
    #[prost(string, tag = "1")]
    pub path: ::prost::alloc::string::String,
    /// permission bits
    #[prost(int32, tag = "2")]
    pub mode: i32,
    /// data is the new file contents
    #[prost(bytes = "vec", tag = "3")]
    pub data: ::prost::alloc::vec::Vec<u8>,
    /// optional owner for the new file
    #[prost(message, optional, tag = "4")]
    pub owner: ::core::option::Option<ChownOpt>,
    /// optional created time override
    #[prost(int64, tag = "5")]
    pub timestamp: i64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FileActionMkDir {
    /// path for the new directory
    #[prost(string, tag = "1")]
    pub path: ::prost::alloc::string::String,
    /// permission bits
    #[prost(int32, tag = "2")]
    pub mode: i32,
    /// makeParents creates parent directories as well if needed
    #[prost(bool, tag = "3")]
    pub make_parents: bool,
    /// optional owner for the new directory
    #[prost(message, optional, tag = "4")]
    pub owner: ::core::option::Option<ChownOpt>,
    /// optional created time override
    #[prost(int64, tag = "5")]
    pub timestamp: i64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FileActionRm {
    /// path to remove
    #[prost(string, tag = "1")]
    pub path: ::prost::alloc::string::String,
    /// allowNotFound doesn't fail the rm if file is not found
    #[prost(bool, tag = "2")]
    pub allow_not_found: bool,
    /// allowWildcard allows filepath.Match wildcards in path
    #[prost(bool, tag = "3")]
    pub allow_wildcard: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ChownOpt {
    #[prost(message, optional, tag = "1")]
    pub user: ::core::option::Option<UserOpt>,
    #[prost(message, optional, tag = "2")]
    pub group: ::core::option::Option<UserOpt>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UserOpt {
    #[prost(oneof = "user_opt::User", tags = "1, 2")]
    pub user: ::core::option::Option<user_opt::User>,
}
/// Nested message and enum types in `UserOpt`.
pub mod user_opt {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum User {
        #[prost(message, tag = "1")]
        ByName(super::NamedUserOpt),
        #[prost(uint32, tag = "2")]
        ById(u32),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NamedUserOpt {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(int64, tag = "2")]
    pub input: i64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MergeInput {
    #[prost(int64, tag = "1")]
    pub input: i64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MergeOp {
    #[prost(message, repeated, tag = "1")]
    pub inputs: ::prost::alloc::vec::Vec<MergeInput>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LowerDiffInput {
    #[prost(int64, tag = "1")]
    pub input: i64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UpperDiffInput {
    #[prost(int64, tag = "1")]
    pub input: i64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DiffOp {
    #[prost(message, optional, tag = "1")]
    pub lower: ::core::option::Option<LowerDiffInput>,
    #[prost(message, optional, tag = "2")]
    pub upper: ::core::option::Option<UpperDiffInput>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum NetMode {
    /// sandbox
    Unset = 0,
    Host = 1,
    None = 2,
}
impl NetMode {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            NetMode::Unset => "UNSET",
            NetMode::Host => "HOST",
            NetMode::None => "NONE",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "UNSET" => Some(Self::Unset),
            "HOST" => Some(Self::Host),
            "NONE" => Some(Self::None),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum SecurityMode {
    Sandbox = 0,
    /// privileged mode
    Insecure = 1,
}
impl SecurityMode {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            SecurityMode::Sandbox => "SANDBOX",
            SecurityMode::Insecure => "INSECURE",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "SANDBOX" => Some(Self::Sandbox),
            "INSECURE" => Some(Self::Insecure),
            _ => None,
        }
    }
}
/// MountType defines a type of a mount from a supported set
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum MountType {
    Bind = 0,
    Secret = 1,
    Ssh = 2,
    Cache = 3,
    Tmpfs = 4,
}
impl MountType {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            MountType::Bind => "BIND",
            MountType::Secret => "SECRET",
            MountType::Ssh => "SSH",
            MountType::Cache => "CACHE",
            MountType::Tmpfs => "TMPFS",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "BIND" => Some(Self::Bind),
            "SECRET" => Some(Self::Secret),
            "SSH" => Some(Self::Ssh),
            "CACHE" => Some(Self::Cache),
            "TMPFS" => Some(Self::Tmpfs),
            _ => None,
        }
    }
}
/// CacheSharingOpt defines different sharing modes for cache mount
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum CacheSharingOpt {
    /// SHARED cache mount can be used concurrently by multiple writers
    Shared = 0,
    /// PRIVATE creates a new mount if there are multiple writers
    Private = 1,
    /// LOCKED pauses second writer until first one releases the mount
    Locked = 2,
}
impl CacheSharingOpt {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            CacheSharingOpt::Shared => "SHARED",
            CacheSharingOpt::Private => "PRIVATE",
            CacheSharingOpt::Locked => "LOCKED",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "SHARED" => Some(Self::Shared),
            "PRIVATE" => Some(Self::Private),
            "LOCKED" => Some(Self::Locked),
            _ => None,
        }
    }
}
