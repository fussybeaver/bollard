use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::Path;

use serde::Deserialize;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceLock {
    pub schema: u32,
    pub moby: Moby,
    pub buildkit: Buildkit,
    #[serde(default, rename = "resource")]
    pub resources: Vec<Resource>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Moby {
    pub tag: String,
    pub commit: String,
    pub go_mod_sha256: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Buildkit {
    pub version: String,
    pub commit: String,
    pub image: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
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

    use super::load;

    const VALID_LOCK: &str = r#"
schema = 1

[moby]
tag = "docker-v29.4.1"
commit = "6c91b92cc71077b70c779c510da125301a8e40f3"
go_mod_sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

[buildkit]
version = "v0.29.0"
commit = "8543ce4428265d547cb009e5ad62348284497a88"
image = "moby/buildkit:v0.29.0"

[[resource]]
destination = "pb/ops.proto"
repository = "moby/buildkit"
revision = "8543ce4428265d547cb009e5ad62348284497a88"
path = "solver/pb/ops.proto"
source_sha256 = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
output_sha256 = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
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
    fn rejects_unknown_fields() {
        let error = load_contents(&format!("{VALID_LOCK}\nextra = true"))
            .unwrap_err();
        assert!(error.contains("unknown field"));
    }

    #[test]
    fn rejects_unsupported_schema() {
        let error = load_contents(&VALID_LOCK.replacen("schema = 1", "schema = 2", 1))
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
        let duplicate = format!("{VALID_LOCK}\n\n[[resource]]\ndestination = \"pb/ops.proto\"\nrepository = \"moby/buildkit\"\nrevision = \"8543ce4428265d547cb009e5ad62348284497a88\"\npath = \"solver/pb/ops.proto\"\nsource_sha256 = \"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"\noutput_sha256 = \"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\"\ntransform = \"none\"\n");
        let error = load_contents(&duplicate).unwrap_err();
        assert!(error.contains("duplicate resource destination"));

        let unsorted = VALID_LOCK.replacen("destination = \"pb/ops.proto\"", "destination = \"z/ops.proto\"", 1);
        let second = format!("{unsorted}\n\n[[resource]]\ndestination = \"a/ops.proto\"\nrepository = \"moby/buildkit\"\nrevision = \"8543ce4428265d547cb009e5ad62348284497a88\"\npath = \"solver/pb/ops.proto\"\nsource_sha256 = \"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"\noutput_sha256 = \"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\"\ntransform = \"none\"\n");
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
