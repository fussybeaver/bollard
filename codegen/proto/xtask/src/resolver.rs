use std::error::Error;
use std::fmt::{Display, Formatter};

use sha2::{Digest, Sha256};

use crate::github::Remote;
use crate::gomod::{parse_buildkit_requirement, BuildkitVersion};
use crate::pom::MobyInputSpec;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedBaseline {
    pub moby_reference: String,
    pub moby_commit: String,
    pub moby_go_mod_sha256: String,
    pub buildkit_version: String,
    pub buildkit_commit: String,
    pub buildkit_image: String,
}

#[derive(Debug)]
struct ResolverError(String);

impl Display for ResolverError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for ResolverError {}

pub fn resolve<R: Remote>(
    remote: &R,
    input_spec: &MobyInputSpec,
) -> Result<ResolvedBaseline> {
    let moby_commit = remote.resolve_tag("moby", "moby", &input_spec.reference)?;
    let go_mod = remote.fetch_raw("moby", "moby", &moby_commit, "go.mod")?;
    let go_mod_sha256 = sha256(&go_mod);
    let go_mod = String::from_utf8(go_mod)
        .map_err(|error| resolver_error(format!("Moby go.mod is not UTF-8: {error}")))?;
    let requirement = parse_buildkit_requirement(&go_mod)?;

    let (buildkit_version, buildkit_commit, buildkit_image) = match requirement.version {
        BuildkitVersion::Tagged(version) => {
            let commit = remote.resolve_tag("moby", "buildkit", &version)?;
            (version.clone(), commit, format!("moby/buildkit:{version}"))
        }
        BuildkitVersion::Pseudo {
            version,
            commit_prefix,
        } => {
            let commit = remote.resolve_commit_prefix("moby", "buildkit", &commit_prefix)?;
            return Err(resolver_error(format!(
                "BuildKit requirement {version} resolves to {commit}; set a reviewed image reference before recording this pseudo-version"
            )));
        }
    };

    Ok(ResolvedBaseline {
        moby_reference: input_spec.reference.clone(),
        moby_commit,
        moby_go_mod_sha256: go_mod_sha256,
        buildkit_version,
        buildkit_commit,
        buildkit_image,
    })
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn resolver_error(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(ResolverError(message.into()))
}

impl Display for ResolvedBaseline {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(formatter, "Moby reference:  {}", self.moby_reference)?;
        writeln!(formatter, "Moby commit:     {}", self.moby_commit)?;
        writeln!(formatter, "Moby go.mod:     sha256:{}", self.moby_go_mod_sha256)?;
        writeln!(formatter, "BuildKit:        {}", self.buildkit_version)?;
        writeln!(formatter, "BuildKit commit: {}", self.buildkit_commit)?;
        write!(formatter, "BuildKit image:  {}", self.buildkit_image)
    }

}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{resolve, ResolvedBaseline};
    use crate::github::Remote;
    use crate::pom::MobyInputSpec;

    struct FakeRemote {
        tags: HashMap<(String, String), String>,
        files: HashMap<&'static str, &'static [u8]>,
    }

    impl Remote for FakeRemote {
        fn resolve_tag(&self, owner: &str, repository: &str, tag: &str) -> super::Result<String> {
            self.tags
                .get(&(repository.into(), tag.into()))
                .cloned()
                .ok_or_else(|| format!("missing fake tag {owner}/{repository}:{tag}").into())
        }

        fn resolve_commit_prefix(
            &self,
            _owner: &str,
            _repository: &str,
            _prefix: &str,
        ) -> super::Result<String> {
            Ok("a2aa163d723fe2c00105350a49e9e2b02242f472".into())
        }

        fn fetch_raw(
            &self,
            _owner: &str,
            _repository: &str,
            revision: &str,
            path: &str,
        ) -> super::Result<Vec<u8>> {
            self.files
                .get(path)
                .map(|contents| contents.to_vec())
                .ok_or_else(|| format!("missing fake file {revision}/{path}").into())
        }
    }

    #[test]
    fn resolves_current_baseline_without_network() {
        let go_mod = b"module github.com/moby/moby/v2\nrequire github.com/moby/buildkit v0.29.0\n";
        let remote = FakeRemote {
            tags: HashMap::from([
                (
                    ("moby".into(), "docker-v29.4.1".into()),
                    "6c91b92cc71077b70c779c510da125301a8e40f3".into(),
                ),
                (
                    ("buildkit".into(), "v0.29.0".into()),
                    "8543ce4428265d547cb009e5ad62348284497a88".into(),
                ),
            ]),
            files: HashMap::from([("go.mod", go_mod.as_slice())]),
        };
        let input_spec = MobyInputSpec {
            url: "https://raw.githubusercontent.com/moby/moby/docker-v29.4.1/api/docs/v1.53.yaml".into(),
            reference: "docker-v29.4.1".into(),
        };

        let baseline = resolve(&remote, &input_spec).unwrap();
        assert_eq!(
            baseline,
            ResolvedBaseline {
                moby_reference: "docker-v29.4.1".into(),
                moby_commit: "6c91b92cc71077b70c779c510da125301a8e40f3".into(),
                moby_go_mod_sha256: "6c6e0b6e82fb19a2fd187e95164534d7b4e046929f512fd6c16d45f66f3d887a".into(),
                buildkit_version: "v0.29.0".into(),
                buildkit_commit: "8543ce4428265d547cb009e5ad62348284497a88".into(),
                buildkit_image: "moby/buildkit:v0.29.0".into(),
            }
        );
    }

    #[test]
    fn rejects_pseudo_version_until_image_is_reviewed() {
        let remote = FakeRemote {
            tags: HashMap::from([(
                ("moby".into(), "docker-v29.4.1".into()),
                "6c91b92cc71077b70c779c510da125301a8e40f3".into(),
            )]),
            files: HashMap::from([(
                "go.mod",
                b"require github.com/moby/buildkit v0.0.0-20251211185533-a2aa163d723f\n".as_slice(),
            )]),
        };
        let input_spec = MobyInputSpec {
            url: "fixture".into(),
            reference: "docker-v29.4.1".into(),
        };
        let error = resolve(&remote, &input_spec).unwrap_err().to_string();
        assert!(error.contains("reviewed image reference"));
    }

}
