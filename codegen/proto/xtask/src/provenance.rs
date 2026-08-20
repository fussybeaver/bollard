use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use tempfile::{NamedTempFile, TempPath};

use crate::resolver::ResolvedBaseline;
use crate::resources::{IndependentPin, PreparedSource};
use crate::support::{validate_commit, validate_path};
use crate::transform;

// The schema version changes only when the lock structure or field semantics
// change. The generation contract changes when the generation recipe changes.
const SCHEMA_VERSION: u32 = 2;
pub const GENERATION_CONTRACT: u32 = 1;
pub const PROTOC_VERSION: &str = "31.1";
pub const PROTOC_BIN_VENDORED_VERSION: &str = "3.2.0";
pub const TONIC_PROST_BUILD_VERSION: &str = "0.14.6";
pub const PROST_BUILD_VERSION: &str = "0.14.4";

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceLock {
    pub schema: u32,
    pub moby: Moby,
    pub buildkit: Buildkit,
    pub generation: Generation,
    #[serde(default, rename = "resource")]
    pub resources: Vec<Resource>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Generation {
    pub contract: u32,
    pub protoc: String,
    pub tonic_prost_build: String,
    pub prost_build: String,
    pub include_root: String,
    pub compile_well_known_types: bool,
    pub btree_map: String,
    pub packet_source: String,
    pub packet_output: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Moby {
    pub tag: String,
    pub commit: String,
    pub go_mod_sha256: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Buildkit {
    pub version: String,
    pub commit: String,
    pub image: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub destination: String,
    pub repository: String,
    pub revision: String,
    pub path: String,
    pub source_sha256: String,
    pub output_sha256: String,
    pub transform: String,
}

#[derive(Debug, Deserialize)]
struct UpdateLock {
    #[serde(default, rename = "resource")]
    resources: Vec<Resource>,
}

pub fn load(path: &Path) -> Result<ProvenanceLock> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("could not read provenance lock {}", path.display()))?;
    let lock: ProvenanceLock = toml::from_str(&contents)
        .with_context(|| format!("could not parse provenance lock {}", path.display()))?;
    lock.validate()?;
    validate_moby_release_tag(&lock.moby.tag)?;
    Ok(lock)
}

pub fn load_update_pins(path: &Path) -> Result<BTreeMap<String, IndependentPin>> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("could not read provenance lock {}", path.display()))?;
    let lock: UpdateLock = toml::from_str(&contents)
        .with_context(|| format!("could not parse provenance lock {}", path.display()))?;
    independent_pins_from_resources(&lock.resources)
}

impl ProvenanceLock {
    pub fn from_prepared(
        baseline: &ResolvedBaseline,
        mut prepared: Vec<PreparedSource>,
    ) -> Result<Self> {
        prepared.sort_by(|left, right| left.destination.cmp(&right.destination));
        let lock = Self {
            schema: SCHEMA_VERSION,
            moby: Moby {
                tag: baseline.moby_reference.clone(),
                commit: baseline.moby_commit.clone(),
                go_mod_sha256: baseline.moby_go_mod_sha256.clone(),
            },
            buildkit: Buildkit {
                version: baseline.buildkit_version.clone(),
                commit: baseline.buildkit_commit.clone(),
                image: baseline.buildkit_image.clone(),
            },
            generation: current_generation(),
            resources: prepared
                .into_iter()
                .map(|source| Resource {
                    destination: source.destination,
                    repository: source.repository,
                    revision: source.revision,
                    path: source.path,
                    source_sha256: source.source_sha256,
                    output_sha256: source.output_sha256,
                    transform: source.transform,
                })
                .collect(),
        };
        lock.validate()?;
        Ok(lock)
    }

    pub fn to_toml(&self) -> Result<String> {
        self.validate()?;
        validate_moby_release_tag(&self.moby.tag)?;
        Ok(format!("{}\n", toml::to_string_pretty(self)?.trim_end()))
    }

    pub fn stage(&self, parent: &Path) -> Result<TempPath> {
        fs::create_dir_all(parent)?;
        let contents = self.to_toml()?;
        let mut temporary = NamedTempFile::new_in(parent)?;
        temporary.write_all(contents.as_bytes())?;
        temporary.as_file().sync_all()?;
        Ok(temporary.into_temp_path())
    }

    pub fn independent_pins(&self) -> Result<BTreeMap<String, IndependentPin>> {
        independent_pins_from_resources(&self.resources)
    }

    pub fn resources(&self) -> &[Resource] {
        &self.resources
    }
}

fn independent_pins_from_resources(
    resources: &[Resource],
) -> Result<BTreeMap<String, IndependentPin>> {
        let mut pins = BTreeMap::new();
        for destination in crate::resources::INDEPENDENT_DESTINATIONS {
            let matches: Vec<_> = resources
                .iter()
                .filter(|resource| resource.destination == *destination)
                .collect();
            let resource = match matches.as_slice() {
                [resource] => resource,
                [] => {
                    return Err(anyhow!(
                        "provenance lock is missing independent resource {destination}"
                    ))
                }
                _ => {
                    return Err(anyhow!(
                        "provenance lock contains duplicate independent resource {destination}"
                    ))
                }
            };
            let (owner, repository) = resource.repository.split_once('/').ok_or_else(|| {
                anyhow!(
                    "resource {destination} repository must be owner/repository"
                )
            })?;
            if owner.is_empty() || repository.is_empty() || repository.contains('/') {
                return Err(anyhow!(
                    "resource {destination} repository must be owner/repository"
                ));
            }
            validate_commit(
                &format!("resource {destination} revision"),
                &resource.revision,
            )?;
            validate_path(&format!("resource {destination} path"), &resource.path)?;
            pins.insert(
                (*destination).to_string(),
                IndependentPin {
                    owner: owner.to_string(),
                    repository: repository.to_string(),
                    revision: resource.revision.clone(),
                    path: resource.path.clone(),
                },
            );
        }
        Ok(pins)
    }

impl ProvenanceLock {
    fn validate(&self) -> Result<()> {
        if self.schema != SCHEMA_VERSION {
            return Err(anyhow!(
                "unsupported provenance lock schema {}; expected {SCHEMA_VERSION}",
                self.schema
            ));
        }

        validate_non_empty("moby.tag", &self.moby.tag)?;
        validate_commit("moby.commit", &self.moby.commit)?;
        validate_sha256("moby.go_mod_sha256", &self.moby.go_mod_sha256)?;

        validate_non_empty("buildkit.version", &self.buildkit.version)?;
        validate_commit("buildkit.commit", &self.buildkit.commit)?;
        validate_non_empty("buildkit.image", &self.buildkit.image)?;
        if self.buildkit.image != format!("moby/buildkit:{}", self.buildkit.version) {
            return Err(anyhow!(
                "buildkit.image must match moby/buildkit:<buildkit.version>",
            ));
        }

        if self.generation != current_generation() {
            return Err(anyhow!(
                "provenance lock generation inputs differ from the pinned xtask toolchain",
            ));
        }

        if self.resources.is_empty() {
            return Err(anyhow!(
                "provenance lock must contain at least one resource",
            ));
        }

        let mut destinations = BTreeSet::new();
        let mut previous_destination = None;
        for (index, resource) in self.resources.iter().enumerate() {
            let field = |name: &str| format!("resource[{index}].{name}");
            validate_path(&field("destination"), &resource.destination)?;
            validate_non_empty(&field("repository"), &resource.repository)?;
            validate_commit(&field("revision"), &resource.revision)?;
            validate_path(&field("path"), &resource.path)?;
            validate_sha256(&field("source_sha256"), &resource.source_sha256)?;
            validate_sha256(&field("output_sha256"), &resource.output_sha256)?;
            validate_non_empty(&field("transform"), &resource.transform)?;
            if resource.transform != transform::for_destination(&resource.destination).name() {
                return Err(anyhow!(
                    "{} records transform {:?}; expected {:?}",
                    field("transform"),
                    resource.transform,
                    transform::for_destination(&resource.destination).name()
                ));
            }
            if resource.transform == "none" && resource.source_sha256 != resource.output_sha256 {
                return Err(anyhow!(
                    "{} with transform none must preserve the source hash",
                    field("output_sha256")
                ));
            }

            if !destinations.insert(&resource.destination) {
                return Err(anyhow!(
                    "duplicate resource destination {:?}",
                    resource.destination
                ));
            }
            if let Some(previous) = previous_destination {
                if previous >= resource.destination.as_str() {
                    return Err(anyhow!(
                        "resource destinations must be strictly sorted; {:?} follows {:?}",
                        resource.destination, previous
                    ));
                }
            }
            previous_destination = Some(resource.destination.as_str());
        }

        Ok(())
    }
}

pub fn current_generation() -> Generation {
    Generation {
        contract: GENERATION_CONTRACT,
        protoc: PROTOC_VERSION.to_string(),
        tonic_prost_build: TONIC_PROST_BUILD_VERSION.to_string(),
        prost_build: PROST_BUILD_VERSION.to_string(),
        include_root: "resources".to_string(),
        compile_well_known_types: true,
        btree_map: ".pb".to_string(),
        packet_source: "moby/filesync/v1/filesync.packet.proto".to_string(),
        packet_output: "moby.filesync.packet.rs".to_string(),
    }
}

fn validate_non_empty(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(anyhow!("{field} must not be empty"));
    }
    Ok(())
}

fn validate_moby_release_tag(value: &str) -> Result<()> {
    let version = value.strip_prefix("docker-v").ok_or_else(|| {
        anyhow!(
            "moby.tag {value:?} must be an immutable docker-v<version> release tag"
        )
    })?;
    semver::Version::parse(version).map_err(|error| {
        anyhow!(
            "moby.tag {value:?} must contain a valid release version: {error}"
        )
    })?;
    Ok(())
}

fn validate_sha256(field: &str, value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(anyhow!(
            "{field} must be a 64-character hexadecimal SHA-256 digest"
        ));
    }
    if value.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return Err(anyhow!(
            "{field} must use lowercase hexadecimal"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{load, load_update_pins, ProvenanceLock};
    use crate::resolver::ResolvedBaseline;
    use crate::resources::{PreparedSource, INDEPENDENT_DESTINATIONS};

    const VALID_LOCK: &str = r#"
schema = 2

[moby]
tag = "docker-v29.4.1"
commit = "6c91b92cc71077b70c779c510da125301a8e40f3"
go_mod_sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

[buildkit]
version = "v0.29.0"
commit = "8543ce4428265d547cb009e5ad62348284497a88"
image = "moby/buildkit:v0.29.0"

[generation]
contract = 1
protoc = "31.1"
tonic_prost_build = "0.14.6"
prost_build = "0.14.4"
include_root = "resources"
compile_well_known_types = true
btree_map = ".pb"
packet_source = "moby/filesync/v1/filesync.packet.proto"
packet_output = "moby.filesync.packet.rs"

[[resource]]
destination = "pb/ops.proto"
repository = "moby/buildkit"
revision = "8543ce4428265d547cb009e5ad62348284497a88"
path = "solver/pb/ops.proto"
source_sha256 = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
output_sha256 = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
transform = "none"
"#;

    fn load_contents(contents: &str) -> Result<(), String> {
        let directory = tempdir().map_err(|error| error.to_string())?;
        let path = directory.path().join("provenance.lock.toml");
        fs::write(&path, contents).map_err(|error| error.to_string())?;
        load(&path).map(|_| ()).map_err(|error| format!("{error:#}"))
    }

    fn independent_resource(destination: &str) -> String {
        format!(
            "\n[[resource]]\ndestination = \"{destination}\"\nrepository = \"google/protobuf\"\nrevision = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\npath = \"{destination}\"\nsource_sha256 = \"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"\noutput_sha256 = \"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"\ntransform = \"none\"\n"
        )
    }

    fn update_lock_contents() -> String {
        let resources = INDEPENDENT_DESTINATIONS
            .iter()
            .map(|destination| independent_resource(destination))
            .collect::<String>();
        format!(
            "{}\n{resources}",
            VALID_LOCK
                .replacen("schema = 2", "schema = 99", 1)
                .replacen("protoc = \"31.1\"", "protoc = \"future\"", 1)
        )
    }

    fn load_update_contents(contents: &str) -> Result<usize, String> {
        let directory = tempdir().map_err(|error| error.to_string())?;
        let path = directory.path().join("provenance.lock.toml");
        fs::write(&path, contents).map_err(|error| error.to_string())?;
        load_update_pins(&path)
            .map(|pins| pins.len())
            .map_err(|error| format!("{error:#}"))
    }

    #[test]
    fn loads_valid_lock() {
        load_contents(VALID_LOCK).unwrap();
    }

    #[test]
    fn update_loads_pins_without_validating_stale_generation() {
        assert_eq!(load_update_contents(&update_lock_contents()).unwrap(), 5);
    }

    #[test]
    fn update_rejects_missing_duplicate_or_malformed_pins() {
        let valid = update_lock_contents();
        let first = independent_resource(INDEPENDENT_DESTINATIONS[0]);
        let missing = valid.replacen(&first, "", 1);
        let duplicate = format!("{valid}\n{first}");
        let malformed = valid.replacen(
            "revision = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"",
            "revision = \"not-a-revision\"",
            1,
        );

        for contents in [missing, duplicate, malformed] {
            assert!(load_update_contents(&contents).is_err());
        }
    }

    #[test]
    fn serializes_and_round_trips_valid_lock() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("provenance.lock.toml");
        fs::write(&path, VALID_LOCK).unwrap();
        let lock = load(&path).unwrap();
        let serialized = lock.to_toml().unwrap();
        let round_trip: ProvenanceLock = toml::from_str(&serialized).unwrap();
        assert_eq!(round_trip, lock);
        assert!(serialized.starts_with("schema = 2\n"));
    }

    #[test]
    fn builds_lock_from_prepared_sources() {
        let baseline = ResolvedBaseline {
            moby_reference: "docker-v29.4.1".into(),
            moby_commit: "6c91b92cc71077b70c779c510da125301a8e40f3".into(),
            moby_go_mod_sha256:
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            buildkit_version: "v0.29.0".into(),
            buildkit_commit: "8543ce4428265d547cb009e5ad62348284497a88".into(),
            buildkit_image: "moby/buildkit:v0.29.0".into(),
        };
        let source = PreparedSource {
            class: crate::resources::DependencyClass::BuildkitOwned,
            destination: "pb/ops.proto".into(),
            repository: "moby/buildkit".into(),
            revision: baseline.buildkit_commit.clone(),
            path: "solver/pb/ops.proto".into(),
            source_sha256: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                .into(),
            output_sha256: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                .into(),
            transform: "none".into(),
        };
        let lock = ProvenanceLock::from_prepared(&baseline, vec![source]).unwrap();
        assert_eq!(lock.resources()[0].destination, "pb/ops.proto");
        assert_eq!(lock.buildkit.image, "moby/buildkit:v0.29.0");
    }

    #[test]
    fn rejects_transform_not_matching_destination() {
        let error = load_contents(&VALID_LOCK.replace(
            "transform = \"none\"",
            "transform = \"adapt-filesend-packet\"",
        ))
        .unwrap_err();
        assert!(error.contains("expected \"none\""));
    }

    #[test]
    fn rejects_unknown_fields() {
        let error = load_contents(&format!("{VALID_LOCK}\nextra = true"))
            .unwrap_err();
        assert!(error.contains("unknown field"));
    }

    #[test]
    fn rejects_generation_toolchain_drift() {
        let error = load_contents(&VALID_LOCK.replacen(
            "protoc = \"31.1\"",
            "protoc = \"31.0\"",
            1,
        ))
        .unwrap_err();
        assert!(error.contains("generation inputs differ"));
    }

    #[test]
    fn rejects_generation_contract_drift() {
        let error = load_contents(&VALID_LOCK.replacen("contract = 1", "contract = 2", 1))
            .unwrap_err();
        assert!(error.contains("generation inputs differ"));
    }

    #[test]
    fn rejects_unsupported_schema() {
        let error = load_contents(&VALID_LOCK.replacen("schema = 2", "schema = 3", 1))
            .unwrap_err();
        assert!(error.contains("unsupported provenance lock schema"));
    }

    #[test]
    fn rejects_empty_required_values() {
        let error = load_contents(&VALID_LOCK.replacen(
            "image = \"moby/buildkit:v0.29.0\"",
            "image = \" \"",
            1,
        ))
        .unwrap_err();
        assert!(error.contains("buildkit.image must not be empty"));
    }

    #[test]
    fn rejects_empty_resource_list() {
        let lock_without_resources = VALID_LOCK.split("[[resource]]").next().unwrap();
        let error = load_contents(lock_without_resources).unwrap_err();
        assert!(error.contains("at least one resource"));
    }

    #[test]
    fn rejects_malformed_revision_and_hash() {
        let malformed_revision = VALID_LOCK.replacen(
            "6c91b92cc71077b70c779c510da125301a8e40f3",
            "not-a-revision",
            1,
        );
        let error = load_contents(&malformed_revision).unwrap_err();
        assert!(error.contains("moby.commit"));

        let malformed_hash = VALID_LOCK.replacen(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "not-a-hash",
            1,
        );
        let error = load_contents(&malformed_hash).unwrap_err();
        assert!(error.contains("resource[0].source_sha256"));
    }

    #[test]
    fn rejects_unsafe_paths() {
        let unsafe_path = VALID_LOCK.replacen("pb/ops.proto", "../ops.proto", 1);
        let error = load_contents(&unsafe_path).unwrap_err();
        assert!(error.contains("resource[0].destination"));
    }

    #[test]
    fn rejects_duplicate_or_unsorted_destinations() {
        let duplicate = format!("{VALID_LOCK}\n\n[[resource]]\ndestination = \"pb/ops.proto\"\nrepository = \"moby/buildkit\"\nrevision = \"8543ce4428265d547cb009e5ad62348284497a88\"\npath = \"solver/pb/ops.proto\"\nsource_sha256 = \"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"\noutput_sha256 = \"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"\ntransform = \"none\"\n");
        let error = load_contents(&duplicate).unwrap_err();
        assert!(error.contains("duplicate resource destination"));

        let unsorted = VALID_LOCK.replacen("destination = \"pb/ops.proto\"", "destination = \"z/ops.proto\"", 1);
        let second = format!("{unsorted}\n\n[[resource]]\ndestination = \"a/ops.proto\"\nrepository = \"moby/buildkit\"\nrevision = \"8543ce4428265d547cb009e5ad62348284497a88\"\npath = \"solver/pb/ops.proto\"\nsource_sha256 = \"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"\noutput_sha256 = \"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"\ntransform = \"none\"\n");
        let error = load_contents(&second).unwrap_err();
        assert!(error.contains("resource destinations must be strictly sorted"));
    }

    #[test]
    fn reports_missing_lock_path() {
        let directory = tempdir().unwrap();
        let error = load(&directory.path().join("missing.lock.toml"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("could not read provenance lock"));
        assert!(error.contains("missing.lock.toml"));
    }
}
