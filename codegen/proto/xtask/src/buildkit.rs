use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::{tempdir_in, TempDir};

use crate::github::Remote;
use crate::{github, gomod, pom, provenance, resolver, resources};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

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
struct ToolError(String);

impl Display for ToolError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for ToolError {}

#[derive(Debug)]
struct Paths {
    workspace_root: PathBuf,
    pom_path: PathBuf,
    llb_oracle_dir: PathBuf,
    llb_golden_dir: PathBuf,
    llb_go_mod_path: PathBuf,
    llb_manifest_path: PathBuf,
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

#[derive(Debug)]
struct StagedLlbOracle {
    _temporary_directory: TempDir,
    oracle_directory: PathBuf,
    golden_directory: PathBuf,
}

pub fn update(allow_moby_branch: bool) -> Result<()> {
    let paths = paths()?;
    let lock = provenance::load(&paths.lock_path)?;
    let independent_pins = lock.independent_pins()?;
    let (baseline, staged_resources) =
        resolve_and_fetch_sources(&paths, allow_moby_branch, &independent_pins)?;
    println!("Resolved BuildKit compatibility baseline:\n{baseline}");
    resources::print_report(&staged_resources.sources);
    let replacement_lock = provenance::ProvenanceLock::from_prepared(
        &baseline,
        staged_resources.sources.clone(),
    )?;
    let generated = generate(&paths, &staged_resources.directory, &replacement_lock)?;
    let staged_oracle = stage_llb_oracle(&paths, &replacement_lock.buildkit.version)?;
    replace_directory(&staged_resources.directory, &paths.resources_dir)?;
    replace_directory(&generated.directory, &paths.generated_dir)?;
    replace_directory(&staged_oracle.oracle_directory, &paths.llb_oracle_dir)?;
    replace_directory(&staged_oracle.golden_directory, &paths.llb_golden_dir)?;
    if allow_moby_branch {
        eprintln!(
            "WARNING: development-only Moby branch update did not replace {}; release provenance remains unchanged",
            display_path(&paths.workspace_root, &paths.lock_path)
        );
    } else {
        replacement_lock.write_atomic(&paths.lock_path)?;
    }
    println!(
        "Updated generated BuildKit bindings in {}",
        display_path(&paths.workspace_root, &paths.generated_dir)
    );
    Ok(())
}

pub fn check(online: bool) -> Result<()> {
    let paths = paths()?;
    let lock = provenance::load(&paths.lock_path)?;
    verify_pom_tag(&paths, &lock)?;
    verify_llb_oracle(&paths, &lock)?;
    verify_lock_inventory(&lock)?;
    verify_checked_in_resources(&paths, &lock)?;
    let generated = generate(&paths, &paths.resources_dir, &lock)?;
    compare_directories(&generated.directory, &paths.generated_dir)?;
    println!("Generated BuildKit bindings are up to date.");

    if online {
        let independent_pins = lock.independent_pins()?;
        let (baseline, staged_resources) =
            resolve_and_fetch_sources(&paths, false, &independent_pins)?;
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
    }

    Ok(())
}

fn resolve_and_fetch_sources(
    paths: &Paths,
    allow_moby_branch: bool,
    independent_pins: &BTreeMap<String, resources::IndependentPin>,
) -> Result<(resolver::ResolvedBaseline, StagedResources)> {
    let input_spec = pom::parse_input_spec(&fs::read_to_string(&paths.pom_path)?)?;
    if allow_moby_branch {
        eprintln!(
            "WARNING: resolving Moby reference {:?} through the commit API; this development-only mode is mutable and is not release provenance",
            input_spec.reference
        );
    }

    let remote = github::GitHubRemote::from_environment();
    let baseline = resolver::resolve(&remote, &input_spec, allow_moby_branch)?;
    let buildkit_go_mod = remote.fetch_raw(
        "moby",
        "buildkit",
        &baseline.buildkit_commit,
        "go.mod",
    )?;
    let buildkit_go_mod = String::from_utf8(buildkit_go_mod)
        .map_err(|error| ToolError(format!("BuildKit go.mod is not UTF-8: {error}")))?;
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

fn verify_llb_oracle(paths: &Paths, lock: &provenance::ProvenanceLock) -> Result<()> {
    verify_llb_oracle_files(
        &paths.llb_go_mod_path,
        &paths.llb_manifest_path,
        &lock.buildkit.version,
    )
}

fn verify_llb_oracle_files(
    go_mod_path: &Path,
    manifest_path: &Path,
    expected_version: &str,
) -> Result<()> {
    let go_mod = fs::read_to_string(go_mod_path).map_err(|error| {
        tool_error(format!(
            "could not read LLB oracle go.mod {}: {error}",
            go_mod_path.display()
        ))
    })?;
    let requirement = gomod::parse_buildkit_requirement(&go_mod).map_err(|error| {
        tool_error(format!(
            "could not validate LLB oracle go.mod {}: {error}",
            go_mod_path.display()
        ))
    })?;
    let gomod::BuildkitVersion::Tagged(version) = requirement.version else {
        return Err(tool_error(format!(
            "LLB oracle go.mod must use tagged BuildKit version {expected_version}; run go mod tidy after updating {}",
            go_mod_path.display()
        )));
    };
    if version != expected_version {
        return Err(tool_error(format!(
            "LLB oracle go.mod records BuildKit {version}, but provenance requires {expected_version}; update {} and run go mod tidy",
            go_mod_path.display()
        )));
    }

    let manifest_contents = fs::read_to_string(manifest_path).map_err(|error| {
        tool_error(format!(
            "could not read LLB golden manifest {}: {error}",
            manifest_path.display()
        ))
    })?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest_contents).map_err(|error| {
        tool_error(format!(
            "could not parse LLB golden manifest {}: {error}",
            manifest_path.display()
        ))
    })?;
    let manifest_version = manifest
        .get("buildkit_version")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            tool_error(format!(
                "LLB golden manifest {} is missing string buildkit_version",
                manifest_path.display()
            ))
        })?;
    if manifest_version != expected_version {
        return Err(tool_error(format!(
            "LLB golden manifest records BuildKit {manifest_version}, but provenance requires {expected_version}; regenerate the LLB goldens",
        )));
    }

    Ok(())
}

fn stage_llb_oracle(paths: &Paths, buildkit_version: &str) -> Result<StagedLlbOracle> {
    let temporary_directory = tempdir_in(&paths.workspace_root)?;
    let oracle_directory = temporary_directory.path().join("llb-parity");
    let golden_directory = temporary_directory.path().join("golden");
    let golden_again_directory = temporary_directory.path().join("golden-again");
    copy_directory(&paths.llb_oracle_dir, &oracle_directory)?;

    run_command(
        &oracle_directory,
        "go",
        &[String::from("mod"), String::from("edit"), "-require=github.com/moby/buildkit@".to_string() + buildkit_version],
    )?;
    run_command(
        &oracle_directory,
        "go",
        &[String::from("mod"), String::from("tidy")],
    )?;
    run_command(
        &oracle_directory,
        "go",
        &[
            String::from("run"),
            String::from("."),
            String::from("-out"),
            golden_directory.display().to_string(),
        ],
    )?;
    run_command(
        &oracle_directory,
        "go",
        &[
            String::from("run"),
            String::from("."),
            String::from("-out"),
            golden_again_directory.display().to_string(),
        ],
    )?;
    compare_directories_named(
        &golden_directory,
        &golden_again_directory,
        "LLB golden output",
    )?;

    Ok(StagedLlbOracle {
        _temporary_directory: temporary_directory,
        oracle_directory,
        golden_directory,
    })
}

fn run_command(current_dir: &Path, program: &str, args: &[String]) -> Result<()> {
    let output = Command::new(program)
        .args(args)
        .current_dir(current_dir)
        .output()
        .map_err(|error| tool_error(format!("could not run {program}: {error}")))?;
    if output.status.success() {
        return Ok(());
    }

    let command = std::iter::once(program.to_string())
        .chain(args.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ");
    Err(tool_error(format!(
        "command `{command}` failed with {}: {}{}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim(),
        if output.stdout.is_empty() {
            String::new()
        } else {
            format!("\n{}", String::from_utf8_lossy(&output.stdout).trim())
        }
    )))
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
                let hash = resources::sha256(contents);
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
        Box::new(ToolError("could not determine proto directory".to_string())) as Box<dyn Error>
    })?;
    let workspace_root = proto_dir.parent().and_then(Path::parent).ok_or_else(|| {
        Box::new(ToolError("could not determine workspace root".to_string())) as Box<dyn Error>
    })?;

    Ok(Paths {
        workspace_root: workspace_root.to_path_buf(),
        pom_path: workspace_root.join("codegen/swagger/pom.xml"),
        llb_oracle_dir: workspace_root.join("codegen/llb-parity"),
        llb_golden_dir: workspace_root.join("llb/testdata/golden"),
        llb_go_mod_path: workspace_root.join("codegen/llb-parity/go.mod"),
        llb_manifest_path: workspace_root.join("llb/testdata/golden/manifest.json"),
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
    compile(&directory, std::slice::from_ref(&packet_proto), resources_directory, false)?;

    let proto_files: Vec<PathBuf> = PROTO_FILES
        .iter()
        .map(|file| resources_directory.join(file))
        .collect();
    compile(&directory, &proto_files, resources_directory, true)?;
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
    btree_map: bool,
) -> Result<()> {
    if env::var_os("PROTOC").is_some() || env::var_os("PROTOC_INCLUDE").is_some() {
        return Err(tool_error(
            "PROTOC and PROTOC_INCLUDE must be unset; xtask uses its pinned vendored protoc",
        ));
    }

    let protoc = pinned_protoc()?;
    let mut config = tonic_prost_build::Config::new();
    config.protoc_executable(protoc);
    let mut builder = tonic_prost_build::configure()
        .out_dir(output_directory)
        .compile_well_known_types(true);
    if btree_map {
        builder = builder.btree_map(".pb");
    }
    builder.compile_with_config(
        config,
        proto_files,
        &[resources_directory.to_path_buf()],
    )?;

    if !btree_map {
        let packet_generated = output_directory.join("moby.filesync.v1.rs");
        let packet_output = output_directory.join("moby.filesync.packet.rs");
        if !packet_generated.exists() {
            return Err(Box::new(ToolError(format!(
                "packet generation did not produce {}",
                packet_generated.display()
            ))));
        }
        fs::copy(packet_generated, packet_output)?;
    }

    Ok(())
}

fn pinned_protoc() -> Result<PathBuf> {
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    let output = Command::new(&protoc).arg("--version").output()?;
    if !output.status.success() {
        return Err(tool_error(format!(
            "pinned protoc failed to report its version: {}",
            protoc.display()
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

fn replace_directory(source: &Path, destination: &Path) -> Result<()> {
    let parent = destination.parent().ok_or_else(|| {
        Box::new(ToolError(format!(
            "directory has no parent: {}",
            destination.display()
        ))) as Box<dyn Error>
    })?;
    fs::create_dir_all(parent)?;

    let staging = tempdir_in(parent)?;
    let staged_destination = staging.path().join("generated");
    copy_directory(source, &staged_destination)?;

    if destination.exists() {
        let backup = staging.path().join("previous");
        fs::rename(destination, &backup)?;
        match fs::rename(&staged_destination, destination) {
            Ok(()) => fs::remove_dir_all(backup)?,
            Err(error) => {
                fs::rename(backup, destination)?;
                return Err(Box::new(error));
            }
        }
    } else {
        fs::rename(staged_destination, destination)?;
    }

    Ok(())
}

fn copy_directory(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_directory(&source_path, &destination_path)?;
        } else {
            fs::copy(source_path, destination_path)?;
        }
    }
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

    Err(Box::new(ToolError(format!(
        "{label} differ: {}",
        differences
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))))
}

fn files(directory: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
    if !directory.is_dir() {
        return Err(Box::new(ToolError(format!(
            "generated directory does not exist: {}",
            directory.display()
        ))));
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

fn tool_error(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(ToolError(message.into()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{
        compare_directories, copy_directory, provenance, replace_directory,
        verify_llb_oracle_files,
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
    fn failed_staging_does_not_replace_existing_output() {
        let temporary_directory = tempdir().unwrap();
        let destination = temporary_directory.path().join("generated");
        fs::create_dir_all(&destination).unwrap();
        fs::write(destination.join("existing.rs"), b"keep").unwrap();

        let missing_source = temporary_directory.path().join("missing");
        assert!(replace_directory(&missing_source, &destination).is_err());
        assert_eq!(fs::read(destination.join("existing.rs")).unwrap(), b"keep");
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
    fn accepts_matching_llb_oracle_versions() {
        let temporary_directory = tempdir().unwrap();
        let go_mod = temporary_directory.path().join("go.mod");
        let manifest = temporary_directory.path().join("manifest.json");
        fs::write(
            &go_mod,
            "module go-parity\n\nrequire github.com/moby/buildkit v0.29.0\n",
        )
        .unwrap();
        fs::write(&manifest, r#"{"buildkit_version":"v0.29.0"}"#).unwrap();

        verify_llb_oracle_files(&go_mod, &manifest, "v0.29.0").unwrap();
    }

    #[test]
    fn rejects_stale_llb_oracle_go_mod() {
        let temporary_directory = tempdir().unwrap();
        let go_mod = temporary_directory.path().join("go.mod");
        let manifest = temporary_directory.path().join("manifest.json");
        fs::write(
            &go_mod,
            "module go-parity\n\nrequire github.com/moby/buildkit v0.31.1\n",
        )
        .unwrap();
        fs::write(&manifest, r#"{"buildkit_version":"v0.29.0"}"#).unwrap();

        let error = verify_llb_oracle_files(&go_mod, &manifest, "v0.29.0").unwrap_err();
        assert!(error.to_string().contains("go.mod records BuildKit v0.31.1"));
    }

    #[test]
    fn rejects_stale_llb_manifest() {
        let temporary_directory = tempdir().unwrap();
        let go_mod = temporary_directory.path().join("go.mod");
        let manifest = temporary_directory.path().join("manifest.json");
        fs::write(
            &go_mod,
            "module go-parity\n\nrequire github.com/moby/buildkit v0.29.0\n",
        )
        .unwrap();
        fs::write(&manifest, r#"{"buildkit_version":"v0.31.1"}"#).unwrap();

        let error = verify_llb_oracle_files(&go_mod, &manifest, "v0.29.0").unwrap_err();
        assert!(error
            .to_string()
            .contains("golden manifest records BuildKit v0.31.1"));
    }
}
