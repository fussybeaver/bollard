use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

use tempfile::{tempdir_in, TempDir};

use crate::provenance;

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
    proto_dir: PathBuf,
    resources_dir: PathBuf,
    generated_dir: PathBuf,
}

#[derive(Debug)]
struct GeneratedOutput {
    _temporary_directory: TempDir,
    directory: PathBuf,
}

pub fn update() -> Result<()> {
    let paths = paths()?;
    let generated = generate(&paths)?;
    replace_generated(&generated.directory, &paths.generated_dir)?;
    println!(
        "Updated generated BuildKit bindings in {}",
        display_path(&paths.workspace_root, &paths.generated_dir)
    );
    Ok(())
}

pub fn check(online: bool) -> Result<()> {
    let paths = paths()?;
    let generated = generate(&paths)?;
    compare_directories(&generated.directory, &paths.generated_dir)?;
    println!("Generated BuildKit bindings are up to date.");

    if online {
        let lock_path = paths.proto_dir.join("provenance.lock.toml");
        provenance::load(&lock_path)?;
        return Err(Box::new(ToolError(format!(
            "online BuildKit provenance source verification is not implemented yet ({})",
            display_path(&paths.workspace_root, &lock_path)
        ))));
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
        resources_dir: proto_dir.join("resources"),
        generated_dir: proto_dir.join("src/generated"),
        proto_dir: proto_dir.to_path_buf(),
    })
}

fn generate(paths: &Paths) -> Result<GeneratedOutput> {
    let temporary_directory = tempdir_in(&paths.proto_dir)?;
    let directory = temporary_directory.path().join("generated");
    fs::create_dir_all(&directory)?;

    let packet_proto = paths.resources_dir.join(PACKET_PROTO);
    let resources = paths.resources_dir.clone();
    tonic_prost_build::configure()
        .out_dir(&directory)
        .compile_well_known_types(true)
        .compile_protos(
            std::slice::from_ref(&packet_proto),
            std::slice::from_ref(&resources),
        )?;

    let packet_generated = directory.join("moby.filesync.v1.rs");
    let packet_output = directory.join("moby.filesync.packet.rs");
    if !packet_generated.exists() {
        return Err(Box::new(ToolError(format!(
            "packet generation did not produce {}",
            packet_generated.display()
        ))));
    }
    fs::copy(packet_generated, packet_output)?;

    let proto_files: Vec<PathBuf> = PROTO_FILES
        .iter()
        .map(|file| paths.resources_dir.join(file))
        .collect();
    tonic_prost_build::configure()
        .out_dir(&directory)
        .compile_well_known_types(true)
        .btree_map(".pb")
        .compile_protos(&proto_files, std::slice::from_ref(&resources))?;

    Ok(GeneratedOutput {
        _temporary_directory: temporary_directory,
        directory,
    })
}

fn replace_generated(source: &Path, destination: &Path) -> Result<()> {
    let parent = destination.parent().ok_or_else(|| {
        Box::new(ToolError(format!(
            "generated output has no parent: {}",
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
        "generated BuildKit bindings differ: {}",
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

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{compare_directories, copy_directory, replace_generated};

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
        assert!(replace_generated(&missing_source, &destination).is_err());
        assert_eq!(fs::read(destination.join("existing.rs")).unwrap(), b"keep");
    }
}
