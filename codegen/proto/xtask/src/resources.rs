use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::github::Remote;
use crate::resolver::ResolvedBaseline;
use crate::transform::{self, Transform};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub const INDEPENDENT_DESTINATIONS: &[&str] = &[
    "google/protobuf/any.proto",
    "google/protobuf/descriptor.proto",
    "google/protobuf/timestamp.proto",
    "google/rpc/status.proto",
    "grpc/health/v1/health.proto",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DependencyClass {
    BuildkitOwned,
    BuildkitVendored,
    Independent,
}

impl Display for DependencyClass {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::BuildkitOwned => "BuildKit-owned",
            Self::BuildkitVendored => "BuildKit-vendored",
            Self::Independent => "independent",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub class: DependencyClass,
    pub destination: String,
    pub owner: String,
    pub repository: String,
    pub revision: String,
    pub path: String,
    pub transform: Transform,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndependentPin {
    pub owner: String,
    pub repository: String,
    pub revision: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedSource {
    pub class: DependencyClass,
    pub destination: String,
    pub repository: String,
    pub revision: String,
    pub path: String,
    pub source_sha256: String,
    pub output_sha256: String,
    pub transform: String,
}

#[derive(Debug)]
struct ResourceError(String);

impl Display for ResourceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for ResourceError {}

pub fn inventory(
    baseline: &ResolvedBaseline,
    vtprotobuf_revision: &str,
    independent_pins: &BTreeMap<String, IndependentPin>,
) -> Result<Vec<Source>> {
    validate_commit(vtprotobuf_revision, "vtprotobuf revision")?;

    let buildkit = |destination: &str, path: &str| Source {
        class: DependencyClass::BuildkitOwned,
        destination: destination.into(),
        owner: "moby".into(),
        repository: "buildkit".into(),
        revision: baseline.buildkit_commit.clone(),
        path: path.into(),
        transform: transform::for_destination(destination),
    };
    let vendored = |destination: &str, path: &str| Source {
        class: DependencyClass::BuildkitVendored,
        destination: destination.into(),
        owner: "moby".into(),
        repository: "buildkit".into(),
        revision: baseline.buildkit_commit.clone(),
        path: path.into(),
        transform: transform::for_destination(destination),
    };
    let independent = |destination: &str| -> Result<Source> {
        let pin = independent_pins.get(destination).ok_or_else(|| {
            resource_error(format!(
                "provenance lock is missing independent resource {destination}"
            ))
        })?;
        Ok(Source {
            class: DependencyClass::Independent,
            destination: destination.into(),
            owner: pin.owner.clone(),
            repository: pin.repository.clone(),
            revision: pin.revision.clone(),
            path: pin.path.clone(),
            transform: transform::for_destination(destination),
        })
    };

    let sources = vec![
        buildkit("moby/buildkit/v1/control.proto", "api/services/control/control.proto"),
        buildkit("moby/buildkit/v1/secrets.proto", "session/secrets/secrets.proto"),
        buildkit("moby/buildkit/v1/sourcepolicy/policy.proto", "sourcepolicy/pb/policy.proto"),
        buildkit("moby/buildkit/v1/ssh.proto", "session/sshforward/ssh.proto"),
        buildkit("moby/buildkit/v1/types/worker.proto", "api/types/worker.proto"),
        buildkit("moby/filesync/v1/auth.proto", "session/auth/auth.proto"),
        buildkit("moby/filesync/v1/filesync.packet.proto", "session/filesync/filesync.proto"),
        buildkit("moby/filesync/v1/filesync.proto", "session/filesync/filesync.proto"),
        buildkit("moby/upload/v1/upload.proto", "session/upload/upload.proto"),
        buildkit("pb/ops.proto", "solver/pb/ops.proto"),
        vendored(
            "fsutil/types/stat.proto",
            "vendor/github.com/tonistiigi/fsutil/types/stat.proto",
        ),
        vendored(
            "fsutil/types/wire.proto",
            "vendor/github.com/tonistiigi/fsutil/types/wire.proto",
        ),
        Source {
            class: DependencyClass::BuildkitVendored,
            destination: "vtproto/vtproto/ext.proto".into(),
            owner: "planetscale".into(),
            repository: "vtprotobuf".into(),
            revision: vtprotobuf_revision.into(),
            path: "include/github.com/planetscale/vtprotobuf/vtproto/ext.proto".into(),
            transform: transform::for_destination("vtproto/vtproto/ext.proto"),
        },
        independent("google/protobuf/any.proto")?,
        independent("google/protobuf/descriptor.proto")?,
        independent("google/protobuf/timestamp.proto")?,
        independent("google/rpc/status.proto")?,
        independent("grpc/health/v1/health.proto")?,
    ];

    validate_inventory(&sources)?;
    Ok(sources)
}

pub fn fetch_sources<R: Remote>(
    remote: &R,
    sources: &[Source],
    staging_directory: &Path,
) -> Result<Vec<PreparedSource>> {
    fs::create_dir_all(staging_directory)?;
    let mut fetched: BTreeMap<(String, String, String, String), Vec<u8>> = BTreeMap::new();
    let mut output = Vec::with_capacity(sources.len());

    for source in sources {
        let key = (
            source.owner.clone(),
            source.repository.clone(),
            source.revision.clone(),
            source.path.clone(),
        );
        let contents = match fetched.get(&key) {
            Some(contents) => contents.clone(),
            None => {
                let contents = remote.fetch_raw(
                    &source.owner,
                    &source.repository,
                    &source.revision,
                    &source.path,
                )?;
                fetched.insert(key, contents.clone());
                contents
            }
        };

        let destination = checked_destination(staging_directory, &source.destination)?;
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let transformed = transform::apply(source.transform, &source.destination, &contents)?;
        let output_path = checked_destination(staging_directory, &source.destination)?;
        fs::write(&output_path, &transformed)?;

        output.push(PreparedSource {
            class: source.class,
            destination: source.destination.clone(),
            repository: format!("{}/{}", source.owner, source.repository),
            revision: source.revision.clone(),
            path: source.path.clone(),
            source_sha256: sha256(&contents),
            output_sha256: sha256(&transformed),
            transform: source.transform.name().into(),
        });
    }

    output.sort_by(|left, right| left.destination.cmp(&right.destination));
    Ok(output)
}

pub fn print_report(sources: &[PreparedSource]) {
    println!("Prepared immutable BuildKit protobuf sources:");
    for source in sources {
        println!(
            "  [{}] {} <- {}/{} @ {} source_sha256:{} output_sha256:{} transform:{}",
            source.class,
            source.destination,
            source.repository,
            source.path,
            source.revision,
            source.source_sha256,
            source.output_sha256,
            source.transform,
        );
    }
}

fn validate_inventory(sources: &[Source]) -> Result<()> {
    let mut destinations = BTreeSet::new();
    for source in sources {
        validate_commit(&source.revision, &format!("{} revision", source.destination))?;
        validate_path(&source.destination, "destination")?;
        validate_path(&source.path, "source path")?;
        if source.transform != transform::for_destination(&source.destination) {
            return Err(resource_error(format!(
                "{} has unexpected transform {}",
                source.destination,
                source.transform.name()
            )));
        }
        if !destinations.insert(&source.destination) {
            return Err(resource_error(format!(
                "duplicate protobuf source destination {:?}",
                source.destination
            )));
        }
    }
    if sources.len() != 18 {
        return Err(resource_error(format!(
            "protobuf source inventory contains {}; expected 18 destinations",
            sources.len()
        )));
    }
    if !sources
        .iter()
        .any(|source| source.class == DependencyClass::BuildkitOwned)
        || !sources
            .iter()
            .any(|source| source.class == DependencyClass::BuildkitVendored)
        || !sources
            .iter()
            .any(|source| source.class == DependencyClass::Independent)
    {
        return Err(resource_error(
            "protobuf source inventory is missing a dependency class",
        ));
    }
    Ok(())
}

fn checked_destination(root: &Path, destination: &str) -> Result<PathBuf> {
    validate_path(destination, "destination")?;
    Ok(root.join(destination))
}

fn validate_path(value: &str, field: &str) -> Result<()> {
    if value.is_empty()
        || value.starts_with('/')
        || value.contains('\\')
        || value
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Err(resource_error(format!(
            "{field} must be a normalized relative path: {value:?}"
        )));
    }
    Ok(())
}

fn validate_commit(value: &str, field: &str) -> Result<()> {
    if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(resource_error(format!(
            "{field} must be a 40-character hexadecimal Git revision"
        )));
    }
    if value.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return Err(resource_error(format!("{field} must use lowercase hexadecimal")));
    }
    Ok(())
}

pub fn sha256(contents: &[u8]) -> String {
    hex::encode(Sha256::digest(contents))
}

fn resource_error(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(ResourceError(message.into()))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::{BTreeMap, HashMap};
    use std::fs;

    use tempfile::tempdir;

    use super::{fetch_sources, inventory, DependencyClass, IndependentPin, INDEPENDENT_DESTINATIONS};
    use crate::github::Remote;
    use crate::resolver::ResolvedBaseline;
    use crate::transform::Transform;

    const BASELINE: ResolvedBaseline = ResolvedBaseline {
        moby_reference: String::new(),
        moby_commit: String::new(),
        moby_go_mod_sha256: String::new(),
        buildkit_version: String::new(),
        buildkit_commit: String::new(),
        buildkit_image: String::new(),
    };

    fn independent_pins() -> BTreeMap<String, IndependentPin> {
        INDEPENDENT_DESTINATIONS
            .iter()
            .map(|destination| {
                (
                    (*destination).to_string(),
                    IndependentPin {
                        owner: "owner".into(),
                        repository: "repository".into(),
                        revision: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                        path: destination.to_string(),
                    },
                )
            })
            .collect()
    }

    #[test]
    fn inventory_has_expected_classes_and_destinations() {
        let baseline = ResolvedBaseline {
            buildkit_commit: "8543ce4428265d547cb009e5ad62348284497a88".into(),
            ..BASELINE
        };
        let sources = inventory(
            &baseline,
            "0393e58bdf106fe0347e554d272a8f2c84d12461",
            &independent_pins(),
        )
        .unwrap();
        assert_eq!(sources.len(), 18);
        assert!(sources.iter().any(|source| source.class == DependencyClass::BuildkitOwned));
        assert!(sources.iter().any(|source| source.class == DependencyClass::BuildkitVendored));
        assert!(sources.iter().any(|source| source.class == DependencyClass::Independent));
        assert!(!sources.iter().any(|source| source.destination.contains("gogo")));
    }

    #[test]
    fn follows_buildkit_and_vtprotobuf_revisions_but_not_independent_pins() {
        let first_baseline = ResolvedBaseline {
            buildkit_commit: "8543ce4428265d547cb009e5ad62348284497a88".into(),
            ..BASELINE
        };
        let second_baseline = ResolvedBaseline {
            buildkit_commit: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            ..BASELINE
        };
        let first = inventory(
            &first_baseline,
            "0393e58bdf106fe0347e554d272a8f2c84d12461",
            &independent_pins(),
        )
        .unwrap();
        let second = inventory(
            &second_baseline,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            &independent_pins(),
        )
        .unwrap();

        for (first, second) in first.iter().zip(second.iter()) {
            if first.class == DependencyClass::Independent {
                assert_eq!(first.revision, second.revision, "{}", first.destination);
            } else if first.destination == "vtproto/vtproto/ext.proto" {
                assert_ne!(first.revision, second.revision);
            } else {
                assert_eq!(first.revision, first_baseline.buildkit_commit);
                assert_eq!(second.revision, second_baseline.buildkit_commit);
            }
        }
    }

    #[test]
    fn fetches_shared_sources_once_and_hashes_bytes() {
        let sources = vec![
            super::Source {
                class: DependencyClass::BuildkitOwned,
                destination: "first.proto".into(),
                owner: "moby".into(),
                repository: "buildkit".into(),
                revision: "8543ce4428265d547cb009e5ad62348284497a88".into(),
                path: "shared.proto".into(),
                transform: Transform::None,
            },
            super::Source {
                class: DependencyClass::BuildkitOwned,
                destination: "second.proto".into(),
                owner: "moby".into(),
                repository: "buildkit".into(),
                revision: "8543ce4428265d547cb009e5ad62348284497a88".into(),
                path: "shared.proto".into(),
                transform: Transform::None,
            },
        ];
        let directory = tempdir().unwrap();
        let remote = FakeRemote {
            files: HashMap::from([("shared.proto", b"source".to_vec())]),
            fetch_count: Cell::new(0),
        };
        let fetched = fetch_sources(&remote, &sources, directory.path()).unwrap();
        assert_eq!(fetched.len(), 2);
        assert_eq!(remote.fetch_count.get(), 1);
        assert_eq!(fs::read(directory.path().join("first.proto")).unwrap(), b"source");
        assert_eq!(fetched[0].source_sha256, fetched[1].source_sha256);
    }

    struct FakeRemote {
        files: HashMap<&'static str, Vec<u8>>,
        fetch_count: Cell<usize>,
    }

    impl Remote for FakeRemote {
        fn resolve_tag(&self, _owner: &str, _repository: &str, _tag: &str) -> super::Result<String> {
            unreachable!()
        }

        fn resolve_commit_prefix(
            &self,
            _owner: &str,
            _repository: &str,
            _prefix: &str,
        ) -> super::Result<String> {
            unreachable!()
        }

        fn fetch_raw(
            &self,
            _owner: &str,
            _repository: &str,
            _revision: &str,
            path: &str,
        ) -> super::Result<Vec<u8>> {
            self.fetch_count.set(self.fetch_count.get() + 1);
            self.files
                .get(path)
                .cloned()
                .ok_or_else(|| format!("missing fixture {path}").into())
        }
    }
}
