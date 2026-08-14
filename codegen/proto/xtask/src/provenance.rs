use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::resolver::ResolvedBaseline;
use crate::resources::{IndependentPin, PreparedSource};
use crate::transform;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

// The schema version changes only when the lock structure or field semantics
// change. The generation contract changes when the generation recipe changes.
const SCHEMA_VERSION: u32 = 2;
pub const GENERATION_CONTRACT: u32 = 1;
pub const PROTOC_VERSION: &str = "31.1";
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

#[derive(Debug)]
struct ProvenanceError(String);

impl Display for ProvenanceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for ProvenanceError {}

pub fn load(path: &Path) -> Result<ProvenanceLock> {
    let contents = fs::read_to_string(path).map_err(|error| {
        ProvenanceError(format!(
            "could not read provenance lock {}: {error}",
            path.display()
        ))
    })?;
    let lock: ProvenanceLock = toml::from_str(&contents).map_err(|error| {
        ProvenanceError(format!(
            "could not parse provenance lock {}: {error}",
            path.display()
        ))
    })?;
    lock.validate()?;
    Ok(lock)
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
        Ok(format!("{}\n", toml::to_string_pretty(self)?))
    }

    pub fn write_atomic(&self, path: &Path) -> Result<()> {
        let parent = path.parent().ok_or_else(|| {
            validation_error(format!("provenance lock has no parent: {}", path.display()))
        })?;
        fs::create_dir_all(parent)?;
        let contents = self.to_toml()?;
        let mut temporary = NamedTempFile::new_in(parent)?;
        temporary.write_all(contents.as_bytes())?;
        temporary.as_file().sync_all()?;
        temporary.persist(path).map_err(|error| {
            validation_error(format!("could not replace provenance lock {}: {error}", path.display()))
        })?;
        Ok(())
    }

    pub fn independent_pins(&self) -> Result<BTreeMap<String, IndependentPin>> {
        let mut pins = BTreeMap::new();
        for destination in crate::resources::INDEPENDENT_DESTINATIONS {
            let resource = self
                .resources
                .iter()
                .find(|resource| resource.destination == *destination)
                .ok_or_else(|| {
                    validation_error(format!(
                        "provenance lock is missing independent resource {destination}"
                    ))
                })?;
            let (owner, repository) = resource.repository.split_once('/').ok_or_else(|| {
                validation_error(format!(
                    "resource {destination} repository must be owner/repository"
                ))
            })?;
            if owner.is_empty() || repository.is_empty() || repository.contains('/') {
                return Err(validation_error(format!(
                    "resource {destination} repository must be owner/repository"
                )));
            }
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

    pub fn resources(&self) -> &[Resource] {
        &self.resources
    }
}

impl ProvenanceLock {
    fn validate(&self) -> Result<()> {
        if self.schema != SCHEMA_VERSION {
            return Err(validation_error(format!(
                "unsupported provenance lock schema {}; expected {SCHEMA_VERSION}",
                self.schema
            )));
        }

        validate_non_empty("moby.tag", &self.moby.tag)?;
        validate_commit("moby.commit", &self.moby.commit)?;
        validate_sha256("moby.go_mod_sha256", &self.moby.go_mod_sha256)?;

        validate_non_empty("buildkit.version", &self.buildkit.version)?;
        validate_commit("buildkit.commit", &self.buildkit.commit)?;
        validate_non_empty("buildkit.image", &self.buildkit.image)?;
        if self.buildkit.image != format!("moby/buildkit:{}", self.buildkit.version) {
            return Err(validation_error(
                "buildkit.image must match moby/buildkit:<buildkit.version>",
            ));
        }

        if self.generation != current_generation() {
            return Err(validation_error(
                "provenance lock generation inputs differ from the pinned xtask toolchain",
            ));
        }

        if self.resources.is_empty() {
            return Err(validation_error(
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
                return Err(validation_error(format!(
                    "{} records transform {:?}; expected {:?}",
                    field("transform"),
                    resource.transform,
                    transform::for_destination(&resource.destination).name()
                )));
            }
            if resource.transform == "none" && resource.source_sha256 != resource.output_sha256 {
                return Err(validation_error(format!(
                    "{} with transform none must preserve the source hash",
                    field("output_sha256")
                )));
            }

            if !destinations.insert(&resource.destination) {
                return Err(validation_error(format!(
                    "duplicate resource destination {:?}",
                    resource.destination
                )));
            }
            if let Some(previous) = previous_destination {
                if previous >= resource.destination.as_str() {
                    return Err(validation_error(format!(
                        "resource destinations must be strictly sorted; {:?} follows {:?}",
                        resource.destination, previous
                    )));
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

fn validation_error(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(ProvenanceError(message.into()))
}

fn validate_non_empty(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(validation_error(format!("{field} must not be empty")));
    }
    Ok(())
}

fn validate_commit(field: &str, value: &str) -> Result<()> {
    if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(validation_error(format!(
            "{field} must be a 40-character hexadecimal Git revision"
        )));
    }
    if value.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return Err(validation_error(format!(
            "{field} must use lowercase hexadecimal"
        )));
    }
    Ok(())
}

fn validate_sha256(field: &str, value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(validation_error(format!(
            "{field} must be a 64-character hexadecimal SHA-256 digest"
        )));
    }
    if value.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return Err(validation_error(format!(
            "{field} must use lowercase hexadecimal"
        )));
    }
    Ok(())
}

fn validate_path(field: &str, value: &str) -> Result<()> {
    validate_non_empty(field, value)?;
    let path = Path::new(value);
    if path.is_absolute()
        || value.contains('\\')
        || value.split('/').any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Err(validation_error(format!(
            "{field} must be a normalized relative path"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{load, ProvenanceLock};
    use crate::resolver::ResolvedBaseline;
    use crate::resources::PreparedSource;

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
        load(&path).map(|_| ()).map_err(|error| error.to_string())
    }

    #[test]
    fn loads_valid_lock() {
        load_contents(VALID_LOCK).unwrap();
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
    fn writes_lock_atomically() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("provenance.lock.toml");
        let lock = load_contents_lock();
        lock.write_atomic(&path).unwrap();
        assert_eq!(load(&path).unwrap(), lock);
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

    fn load_contents_lock() -> ProvenanceLock {
        let directory = tempdir().unwrap();
        let path = directory.path().join("provenance.lock.toml");
        fs::write(&path, VALID_LOCK).unwrap();
        load(&path).unwrap()
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
