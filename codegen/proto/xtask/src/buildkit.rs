use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use fs_extra::dir::{copy, CopyOptions};
use tempfile::{tempdir_in, TempDir};

use crate::github::Remote;
use crate::{github, gomod, pom, provenance, resolver, resources};
use crate::support::{sha256, xtask_error as tool_error, Result};

const PACKET_PROTO: &str = "moby/filesync/v1/filesync.packet.proto";

const PROTO_FILES: &[&str] = &[
    "fsutil/types/stat.proto",
    "fsutil/types/wire.proto",
    "google/protobuf/any.proto",
    "google/protobuf/timestamp.proto",
    "google/rpc/status.proto",
    "moby/buildkit/v1/control.proto",
    "moby/buildkit/v1/secrets.proto",
    "moby/buildkit/v1/ssh.proto",
    "moby/buildkit/v1/types/worker.proto",
    "moby/buildkit/v1/sourcepolicy/policy.proto",
    "moby/filesync/v1/auth.proto",
    "moby/filesync/v1/filesync.proto",
    "moby/upload/v1/upload.proto",
    "grpc/health/v1/health.proto",
    "pb/ops.proto",
];

#[derive(Debug)]
struct Paths {
    workspace_root: PathBuf,
    pom_path: PathBuf,
    xtask_manifest_path: PathBuf,
    proto_dir: PathBuf,
    resources_dir: PathBuf,
    generated_dir: PathBuf,
    lock_path: PathBuf,
}

#[derive(Debug)]
struct GeneratedOutput {
    _temporary_directory: TempDir,
    directory: PathBuf,
}

#[derive(Debug)]
struct StagedResources {
    _temporary_directory: TempDir,
    directory: PathBuf,
    sources: Vec<resources::PreparedSource>,
}

pub fn update() -> Result<()> {
    let paths = paths()?;
    verify_generator_dependencies(&paths)?;
    let lock = provenance::load(&paths.lock_path)?;
    let independent_pins = lock.independent_pins()?;
    let (baseline, staged_resources) = resolve_and_fetch_sources(&paths, &independent_pins)?;
    println!("Resolved BuildKit compatibility baseline:\n{baseline}");
    resources::print_report(&staged_resources.sources);
    let replacement_lock = provenance::ProvenanceLock::from_prepared(
        &baseline,
        staged_resources.sources.clone(),
    )?;
    let generated = generate(&paths, &staged_resources.directory, &replacement_lock)?;
    let mut commit = OutputTransaction::new();
    commit.add(&staged_resources.directory, &paths.resources_dir)?;
    commit.add(&generated.directory, &paths.generated_dir)?;
    let lock_parent = paths.lock_path.parent().ok_or_else(|| {
        tool_error(format!(
            "provenance lock has no parent: {}",
            paths.lock_path.display()
        ))
    })?;
    let staged_lock = replacement_lock.stage(lock_parent)?;
    commit.add(&staged_lock, &paths.lock_path)?;
    commit.commit()?;
    println!(
        "Updated generated BuildKit bindings in {}",
        display_path(&paths.workspace_root, &paths.generated_dir)
    );
    Ok(())
}

pub fn check() -> Result<()> {
    check_common()?;
    Ok(())
}

pub fn check_online() -> Result<()> {
    let (paths, lock) = check_common()?;
    let independent_pins = lock.independent_pins()?;
    let (baseline, staged_resources) = resolve_and_fetch_sources(&paths, &independent_pins)?;
    verify_baseline(&baseline, &lock)?;
    verify_prepared_sources(&staged_resources.sources, &lock)?;
    compare_directories_named(
        &staged_resources.directory,
        &paths.resources_dir,
        "protobuf resources",
    )?;
    println!("Resolved BuildKit compatibility baseline:\n{baseline}");
    resources::print_report(&staged_resources.sources);
    println!(
        "Verified immutable sources and provenance lock for {}.",
        baseline.buildkit_version
    );
    Ok(())
}

fn check_common() -> Result<(Paths, provenance::ProvenanceLock)> {
    let paths = paths()?;
    let lock = provenance::load(&paths.lock_path)?;
    verify_generator_dependencies(&paths)?;
    verify_pom_tag(&paths, &lock)?;
    verify_lock_inventory(&lock)?;
    verify_checked_in_resources(&paths, &lock)?;
    let generated = generate(&paths, &paths.resources_dir, &lock)?;
    compare_directories(&generated.directory, &paths.generated_dir)?;
    println!("Generated BuildKit bindings are up to date.");
    Ok((paths, lock))
}

fn resolve_and_fetch_sources(
    paths: &Paths,
    independent_pins: &BTreeMap<String, resources::IndependentPin>,
) -> Result<(resolver::ResolvedBaseline, StagedResources)> {
    let input_spec = pom::parse_input_spec(&fs::read_to_string(&paths.pom_path)?)?;

    let remote = github::GitHubRemote::from_environment();
    let baseline = resolver::resolve(&remote, &input_spec)?;
    let buildkit_go_mod = remote.fetch_raw(
        "moby",
        "buildkit",
        &baseline.buildkit_commit,
        "go.mod",
    )?;
    let buildkit_go_mod = String::from_utf8(buildkit_go_mod)
        .map_err(|source_error| tool_error(format!("BuildKit go.mod is not UTF-8: {source_error}")))?;
    let vtprotobuf_revision = gomod::resolve_vtprotobuf_revision(&remote, &buildkit_go_mod)?;
    let inventory = resources::inventory(&baseline, &vtprotobuf_revision, independent_pins)?;
    let temporary_directory = tempdir_in(&paths.proto_dir)?;
    let directory = temporary_directory.path().join("resources");
    let sources = resources::fetch_sources(&remote, &inventory, &directory)?;
    Ok((
        baseline,
        StagedResources {
            _temporary_directory: temporary_directory,
            directory,
            sources,
        },
    ))
}

fn verify_pom_tag(paths: &Paths, lock: &provenance::ProvenanceLock) -> Result<()> {
    let input_spec = pom::parse_input_spec(&fs::read_to_string(&paths.pom_path)?)?;
    if input_spec.reference != lock.moby.tag {
        return Err(tool_error(format!(
            "pom.xml selects {:?}, but provenance lock records {:?}; run cargo xtask buildkit update",
            input_spec.reference, lock.moby.tag
        )));
    }
    Ok(())
}

fn verify_generator_dependencies(paths: &Paths) -> Result<()> {
    verify_generator_dependencies_at(&paths.xtask_manifest_path)
}

fn verify_generator_dependencies_at(path: &Path) -> Result<()> {
    let contents = fs::read_to_string(path).map_err(|error| {
        tool_error(format!(
            "could not read xtask manifest {}: {error}",
            path.display()
        ))
    })?;
    let manifest: toml::Value = toml::from_str(&contents).map_err(|error| {
        tool_error(format!(
            "could not parse xtask manifest {}: {error}",
            path.display()
        ))
    })?;
    let dependencies = manifest
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| tool_error("xtask manifest is missing [dependencies]"))?;

    for (name, expected) in [
        ("protoc-bin-vendored", provenance::PROTOC_BIN_VENDORED_VERSION),
        ("tonic-prost-build", provenance::TONIC_PROST_BUILD_VERSION),
        ("prost-build", provenance::PROST_BUILD_VERSION),
    ] {
        let dependency = dependencies
            .get(name)
            .ok_or_else(|| tool_error(format!("xtask manifest is missing {name}")))?;
        let requirement = dependency
            .as_str()
            .map(str::to_string)
            .or_else(|| {
                dependency
                    .get("version")
                    .and_then(toml::Value::as_str)
                    .map(str::to_string)
            })
            .ok_or_else(|| tool_error(format!("xtask dependency {name} has no version")))?;
        let expected = format!("={expected}");
        if requirement != expected {
            return Err(tool_error(format!(
                "xtask dependency {name} uses {requirement:?}, expected exact requirement {expected:?}"
            )));
        }
    }

    Ok(())
}

fn verify_lock_inventory(lock: &provenance::ProvenanceLock) -> Result<()> {
    let independent_pins = lock.independent_pins()?;
    let vtprotobuf_revision = lock
        .resources()
        .iter()
        .find(|resource| resource.destination == "vtproto/vtproto/ext.proto")
        .ok_or_else(|| tool_error("provenance lock is missing vtprotobuf ext.proto"))?
        .revision
        .clone();
    let baseline = resolver::ResolvedBaseline {
        moby_reference: lock.moby.tag.clone(),
        moby_commit: lock.moby.commit.clone(),
        moby_go_mod_sha256: lock.moby.go_mod_sha256.clone(),
        buildkit_version: lock.buildkit.version.clone(),
        buildkit_commit: lock.buildkit.commit.clone(),
        buildkit_image: lock.buildkit.image.clone(),
    };
    let inventory = resources::inventory(&baseline, &vtprotobuf_revision, &independent_pins)?;
    if inventory.len() != lock.resources().len() {
        return Err(tool_error(format!(
            "provenance lock contains {} resources, but the canonical inventory contains {}; run cargo xtask buildkit update",
            lock.resources().len(),
            inventory.len()
        )));
    }

    for source in inventory {
        let resource = lock
            .resources()
            .iter()
            .find(|resource| resource.destination == source.destination)
            .ok_or_else(|| {
                tool_error(format!(
                    "provenance lock is missing resource {}; run cargo xtask buildkit update",
                    source.destination
                ))
            })?;
        let repository = format!("{}/{}", source.owner, source.repository);
        if resource.repository != repository
            || resource.revision != source.revision
            || resource.path != source.path
            || resource.transform != source.transform.name()
        {
            return Err(tool_error(format!(
                "provenance resource {} does not match the canonical inventory; run cargo xtask buildkit update",
                source.destination
            )));
        }
    }
    Ok(())
}

fn verify_checked_in_resources(
    paths: &Paths,
    lock: &provenance::ProvenanceLock,
) -> Result<()> {
    let actual = files(&paths.resources_dir)?;
    let expected: BTreeMap<PathBuf, &provenance::Resource> = lock
        .resources()
        .iter()
        .map(|resource| (PathBuf::from(&resource.destination), resource))
        .collect();

    let mut mismatches = Vec::new();
    for (path, resource) in &expected {
        match actual.get(path) {
            Some(contents) => {
                let hash = sha256(contents);
                if hash != resource.output_sha256 {
                    mismatches.push(format!(
                        "{} hash {} differs from lock {}",
                        path.display(), hash, resource.output_sha256
                    ));
                }
            }
            None => mismatches.push(format!("missing resource {}", path.display())),
        }
    }
    for path in actual.keys().filter(|path| !expected.contains_key(*path)) {
        mismatches.push(format!("extra resource {}", path.display()));
    }

    if mismatches.is_empty() {
        Ok(())
    } else {
        Err(tool_error(format!(
            "checked-in protobuf resources do not match provenance lock: {}; run cargo xtask buildkit update",
            mismatches.join(", ")
        )))
    }
}

fn verify_baseline(
    baseline: &resolver::ResolvedBaseline,
    lock: &provenance::ProvenanceLock,
) -> Result<()> {
    if baseline.moby_reference != lock.moby.tag
        || baseline.moby_commit != lock.moby.commit
        || baseline.moby_go_mod_sha256 != lock.moby.go_mod_sha256
        || baseline.buildkit_version != lock.buildkit.version
        || baseline.buildkit_commit != lock.buildkit.commit
        || baseline.buildkit_image != lock.buildkit.image
    {
        return Err(tool_error(
            "resolved compatibility baseline differs from provenance lock; run cargo xtask buildkit update",
        ));
    }
    Ok(())
}

fn verify_prepared_sources(
    sources: &[resources::PreparedSource],
    lock: &provenance::ProvenanceLock,
) -> Result<()> {
    let prepared: BTreeMap<&str, &resources::PreparedSource> = sources
        .iter()
        .map(|source| (source.destination.as_str(), source))
        .collect();
    for resource in lock.resources() {
        let source = prepared.get(resource.destination.as_str()).ok_or_else(|| {
            tool_error(format!(
                "online source preparation is missing {}; run cargo xtask buildkit update",
                resource.destination
            ))
        })?;
        if source.repository != resource.repository
            || source.revision != resource.revision
            || source.path != resource.path
            || source.source_sha256 != resource.source_sha256
            || source.output_sha256 != resource.output_sha256
            || source.transform != resource.transform
        {
            return Err(tool_error(format!(
                "online source verification failed for {}; run cargo xtask buildkit update",
                resource.destination
            )));
        }
    }
    if prepared.len() != lock.resources().len() {
        return Err(tool_error(
            "online source preparation contains an unexpected resource set; run cargo xtask buildkit update",
        ));
    }
    Ok(())
}

fn paths() -> Result<Paths> {
    let xtask_directory = Path::new(env!("CARGO_MANIFEST_DIR"));
    let proto_dir = xtask_directory.parent().ok_or_else(|| {
        tool_error("could not determine proto directory")
    })?;
    let workspace_root = proto_dir.parent().and_then(Path::parent).ok_or_else(|| {
        tool_error("could not determine workspace root")
    })?;

    Ok(Paths {
        workspace_root: workspace_root.to_path_buf(),
        pom_path: workspace_root.join("codegen/swagger/pom.xml"),
        xtask_manifest_path: proto_dir.join("xtask/Cargo.toml"),
        resources_dir: proto_dir.join("resources"),
        generated_dir: proto_dir.join("src/generated"),
        proto_dir: proto_dir.to_path_buf(),
        lock_path: proto_dir.join("provenance.lock.toml"),
    })
}

fn generate(
    paths: &Paths,
    resources_directory: &Path,
    lock: &provenance::ProvenanceLock,
) -> Result<GeneratedOutput> {
    let temporary_directory = tempdir_in(&paths.proto_dir)?;
    let directory = temporary_directory.path().join("generated");
    fs::create_dir_all(&directory)?;

    let packet_proto = resources_directory.join(PACKET_PROTO);
    compile(&directory, std::slice::from_ref(&packet_proto), resources_directory)?;
    copy_packet_output(&directory)?;

    let proto_files: Vec<PathBuf> = PROTO_FILES
        .iter()
        .map(|file| resources_directory.join(file))
        .collect();
    compile(&directory, &proto_files, resources_directory)?;
    write_provenance_module(&directory, lock)?;

    Ok(GeneratedOutput {
        _temporary_directory: temporary_directory,
        directory,
    })
}

fn write_provenance_module(
    output_directory: &Path,
    lock: &provenance::ProvenanceLock,
) -> Result<()> {
    let ops_proto_sha256 = lock
        .resources()
        .iter()
        .find(|resource| resource.destination == "pb/ops.proto")
        .map(|resource| resource.output_sha256.as_str())
        .ok_or_else(|| tool_error("provenance lock is missing pb/ops.proto"))?;
    let moby_tag = serde_json::to_string(&lock.moby.tag)?;
    let buildkit_version = serde_json::to_string(&lock.buildkit.version)?;
    let buildkit_commit = serde_json::to_string(&lock.buildkit.commit)?;
    let default_image = serde_json::to_string(&lock.buildkit.image)?;
    let ops_proto_sha256 = serde_json::to_string(ops_proto_sha256)?;
    let contents = format!(
        "// @generated by cargo xtask buildkit; do not edit.\n\n\
         /// Moby source tag used for this compatibility baseline.\n\
         pub const MOBY_TAG: &str = {moby_tag};\n\
         /// BuildKit version used for this compatibility baseline.\n\
         pub const BUILDKIT_VERSION: &str = {buildkit_version};\n\
         /// BuildKit source commit used for this compatibility baseline.\n\
         pub const BUILDKIT_COMMIT: &str = {buildkit_commit};\n\
         /// Default BuildKit image used by the Docker container driver.\n\
         pub const DEFAULT_IMAGE: &str = {default_image};\n\
         /// SHA-256 of the transformed ops.proto consumed by Prost.\n\
         pub const OPS_PROTO_SHA256: &str = {ops_proto_sha256};\n"
    );
    fs::write(output_directory.join("provenance.rs"), contents)?;
    Ok(())
}

fn compile(
    output_directory: &Path,
    proto_files: &[PathBuf],
    resources_directory: &Path,
) -> Result<()> {
    if env::var_os("PROTOC").is_some() || env::var_os("PROTOC_INCLUDE").is_some() {
        return Err(tool_error(
            "PROTOC and PROTOC_INCLUDE must be unset; xtask uses its pinned vendored protoc",
        ));
    }

    let protoc = pinned_protoc()?;
    let mut config = tonic_prost_build::Config::new();
    config.protoc_executable(protoc);
    let builder = tonic_prost_build::configure()
        .out_dir(output_directory)
        .compile_well_known_types(true)
        .btree_map(".pb");
    builder.compile_with_config(
        config,
        proto_files,
        &[resources_directory.to_path_buf()],
    )?;

    Ok(())
}

fn copy_packet_output(output_directory: &Path) -> Result<()> {
    let packet_generated = output_directory.join("moby.filesync.v1.rs");
    let packet_output = output_directory.join("moby.filesync.packet.rs");
    if !packet_generated.exists() {
        return Err(tool_error(format!(
            "packet generation did not produce {}",
            packet_generated.display()
        )));
    }
    fs::copy(packet_generated, packet_output)?;
    Ok(())
}

fn pinned_protoc() -> Result<PathBuf> {
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    let output = Command::new(&protoc)
        .arg("--version")
        .output()
        .map_err(|error| {
            tool_error(format!(
                "pinned protoc failed to report its version: {} ({error})",
                protoc.display()
            ))
        })?;
    if !output.status.success() {
        return Err(tool_error(format!(
            "pinned protoc failed to report its version: {}: {}",
            protoc.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let version = String::from_utf8(output.stdout)?.trim().to_string();
    let expected = format!("libprotoc {}", provenance::PROTOC_VERSION);
    if version != expected {
        return Err(tool_error(format!(
            "vendored protoc reports {version:?}, but provenance requires {expected:?}"
        )));
    }
    Ok(protoc)
}

struct TransactionEntry {
    _staging: TempDir,
    staged: PathBuf,
    destination: PathBuf,
    backup: PathBuf,
    touched: bool,
    installed: bool,
}

/// Rollback-protected replacement of all generated outputs.
///
/// This is atomic for errors returned while this process is running. A process
/// crash or forced termination between renames can still leave mixed outputs.
struct OutputTransaction {
    entries: Vec<TransactionEntry>,
    armed: bool,
}

impl OutputTransaction {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            armed: true,
        }
    }

    fn add(&mut self, source: &Path, destination: &Path) -> Result<()> {
        let parent = destination.parent().ok_or_else(|| {
            tool_error(format!(
                "directory has no parent: {}",
                destination.display()
            ))
        })?;
        fs::create_dir_all(parent)?;
        let staging = tempdir_in(parent)?;
        let staged = staging.path().join("replacement");
        copy_entry(source, &staged)?;
        self.entries.push(TransactionEntry {
            backup: staging.path().join("previous"),
            _staging: staging,
            staged,
            destination: destination.to_path_buf(),
            touched: false,
            installed: false,
        });
        Ok(())
    }

    fn commit(mut self) -> Result<()> {
        self.commit_inner(None)?;
        self.armed = false;
        Ok(())
    }

    #[cfg(test)]
    fn commit_with_failure(mut self, fail_at: usize) -> Result<()> {
        self.commit_inner(Some(fail_at))?;
        self.armed = false;
        Ok(())
    }

    fn commit_inner(&mut self, fail_at: Option<usize>) -> Result<()> {
        for (index, entry) in self.entries.iter_mut().enumerate() {
            if fail_at == Some(index) {
                return Err(Box::new(std::io::Error::other(
                    "injected replacement failure",
                )));
            }

            if fs::symlink_metadata(&entry.destination).is_ok() {
                fs::rename(&entry.destination, &entry.backup)?;
                entry.touched = true;
            }
            fs::rename(&entry.staged, &entry.destination)?;
            entry.installed = true;
        }
        Ok(())
    }
}

impl Drop for OutputTransaction {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }

        for entry in self.entries.iter_mut().rev() {
            if entry.installed {
                if let Err(error) = remove_path(&entry.destination) {
                    eprintln!(
                        "could not remove {} during rollback: {error}",
                        entry.destination.display()
                    );
                }
                entry.installed = false;
            }
            if entry.touched {
                if let Err(error) = fs::rename(&entry.backup, &entry.destination) {
                    eprintln!(
                        "could not restore {} during rollback: {error}",
                        entry.destination.display()
                    );
                }
                entry.touched = false;
            }
        }
    }
}

fn copy_entry(source: &Path, destination: &Path) -> Result<()> {
    if source.is_dir() {
        copy_directory(source, destination)
    } else {
        fs::copy(source, destination)?;
        let file = fs::OpenOptions::new().write(true).open(destination)?;
        file.sync_all()?;
        Ok(())
    }
}

fn remove_path(path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn copy_directory(source: &Path, destination: &Path) -> Result<()> {
    copy(
        source,
        destination,
        &CopyOptions::new().content_only(true),
    )?;
    Ok(())
}

fn compare_directories(actual: &Path, expected: &Path) -> Result<()> {
    compare_directories_named(actual, expected, "generated BuildKit bindings")
}

fn compare_directories_named(actual: &Path, expected: &Path, label: &str) -> Result<()> {
    let actual_files = files(actual)?;
    let expected_files = files(expected)?;
    if actual_files == expected_files {
        return Ok(());
    }

    let mut differences = Vec::new();
    for path in actual_files.keys().chain(expected_files.keys()) {
        if actual_files.get(path) != expected_files.get(path) && !differences.contains(path) {
            differences.push(path.clone());
        }
    }
    differences.sort();

    Err(tool_error(format!(
        "{label} differ: {}",
        differences
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    )))
}

fn files(directory: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
    if !directory.is_dir() {
        return Err(tool_error(format!(
            "generated directory does not exist: {}",
            directory.display()
        )));
    }

    let mut output = BTreeMap::new();
    collect_files(directory, directory, &mut output)?;
    Ok(output)
}

fn collect_files(
    root: &Path,
    directory: &Path,
    output: &mut BTreeMap<PathBuf, Vec<u8>>,
) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, output)?;
        } else {
            output.insert(path.strip_prefix(root)?.to_path_buf(), fs::read(path)?);
        }
    }
    Ok(())
}

fn display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{
        OutputTransaction, compare_directories, copy_directory, provenance, verify_generator_dependencies_at,
    };

    #[test]
    fn compares_identical_directories() {
        let temporary_directory = tempdir().unwrap();
        let first = temporary_directory.path().join("first");
        let second = temporary_directory.path().join("second");
        fs::create_dir_all(first.join("nested")).unwrap();
        fs::write(first.join("nested/output.rs"), b"generated").unwrap();
        copy_directory(&first, &second).unwrap();

        compare_directories(&first, &second).unwrap();
    }

    #[test]
    fn reports_changed_missing_and_extra_files() {
        let temporary_directory = tempdir().unwrap();
        let actual = temporary_directory.path().join("actual");
        let expected = temporary_directory.path().join("expected");
        fs::create_dir_all(&actual).unwrap();
        fs::create_dir_all(&expected).unwrap();
        fs::write(actual.join("changed.rs"), b"new").unwrap();
        fs::write(expected.join("changed.rs"), b"old").unwrap();
        fs::write(actual.join("extra.rs"), b"extra").unwrap();
        fs::write(expected.join("missing.rs"), b"missing").unwrap();

        let error = compare_directories(&actual, &expected).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("changed.rs"));
        assert!(message.contains("extra.rs"));
        assert!(message.contains("missing.rs"));
    }

    #[test]
    fn failed_commit_restores_all_previous_outputs() {
        let temporary_directory = tempdir().unwrap();
        let mut replacements = Vec::new();
        for name in ["resources", "generated"] {
            let source = temporary_directory.path().join(format!("{name}-source"));
            let destination = temporary_directory.path().join(name);
            fs::create_dir_all(&source).unwrap();
            fs::write(source.join("output"), format!("new-{name}")).unwrap();
            fs::create_dir_all(&destination).unwrap();
            fs::write(destination.join("output"), format!("old-{name}")).unwrap();
            replacements.push((source, destination));
        }

        let lock_source = temporary_directory.path().join("lock-source");
        let lock_destination = temporary_directory.path().join("provenance.lock.toml");
        fs::write(&lock_source, b"new-lock").unwrap();
        fs::write(&lock_destination, b"old-lock").unwrap();
        replacements.push((lock_source, lock_destination.clone()));

        let mut commit = OutputTransaction::new();
        for (source, destination) in &replacements {
            commit.add(source, destination).unwrap();
        }
        assert!(commit.commit_with_failure(2).is_err());
        for name in ["resources", "generated"] {
            assert_eq!(
                fs::read(temporary_directory.path().join(name).join("output")).unwrap(),
                format!("old-{name}").as_bytes()
            );
        }
        assert_eq!(fs::read(lock_destination).unwrap(), b"old-lock");
    }

    #[test]
    fn failed_commit_removes_new_outputs() {
        let temporary_directory = tempdir().unwrap();
        let mut replacements = Vec::new();
        for name in ["first", "second"] {
            let source = temporary_directory.path().join(format!("{name}-source"));
            let destination = temporary_directory.path().join(name);
            fs::create_dir_all(&source).unwrap();
            fs::write(source.join("output"), b"new").unwrap();
            replacements.push((source, destination));
        }

        let mut commit = OutputTransaction::new();
        for (source, destination) in &replacements {
            commit.add(source, destination).unwrap();
        }
        assert!(commit.commit_with_failure(1).is_err());
        assert!(!replacements[0].1.exists());
        assert!(!replacements[1].1.exists());
    }

    #[test]
    fn renders_provenance_module_from_lock() {
        let lock_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("provenance.lock.toml");
        let lock = provenance::load(&lock_path).unwrap();
        let temporary_directory = tempdir().unwrap();

        super::write_provenance_module(temporary_directory.path(), &lock).unwrap();

        let generated = fs::read_to_string(temporary_directory.path().join("provenance.rs"))
            .unwrap();
        assert!(generated.contains("pub const MOBY_TAG: &str = \"docker-v29.4.1\";"));
        assert!(generated.contains("pub const BUILDKIT_VERSION: &str = \"v0.29.0\";"));
        assert!(generated.contains(
            "pub const BUILDKIT_COMMIT: &str = \"8543ce4428265d547cb009e5ad62348284497a88\";"
        ));
        assert!(generated.contains("pub const DEFAULT_IMAGE: &str = \"moby/buildkit:v0.29.0\";"));
        assert!(generated.contains(
            "pub const OPS_PROTO_SHA256: &str = \"b45049a4ae961e1eda9acf3834263cf7894ba194d721d5aed04f43b638056c37\";"
        ));
    }

    #[test]
    fn accepts_exact_generator_dependencies() {
        let directory = tempdir().unwrap();
        let manifest = directory.path().join("Cargo.toml");
        fs::write(
            &manifest,
            format!(
                "[dependencies]\nprotoc-bin-vendored = \"={}\"\ntonic-prost-build = \"={}\"\nprost-build = \"={}\"\n",
                provenance::PROTOC_BIN_VENDORED_VERSION,
                provenance::TONIC_PROST_BUILD_VERSION,
                provenance::PROST_BUILD_VERSION,
            ),
        )
        .unwrap();

        verify_generator_dependencies_at(&manifest).unwrap();
    }

    #[test]
    fn rejects_non_exact_generator_dependencies() {
        let directory = tempdir().unwrap();
        let manifest = directory.path().join("Cargo.toml");
        fs::write(
            &manifest,
            "[dependencies]\nprotoc-bin-vendored = \"3.2\"\ntonic-prost-build = \"=0.14.6\"\nprost-build = \"=0.14.4\"\n",
        )
        .unwrap();

        let error = verify_generator_dependencies_at(&manifest).unwrap_err();
        assert!(error.to_string().contains("protoc-bin-vendored"));
        assert!(error.to_string().contains("exact requirement"));
    }
}
