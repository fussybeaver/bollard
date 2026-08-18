use std::str::FromStr;

use gomod_parser::GoMod;
use semver::Version;

use crate::github::Remote;
use crate::support::{xtask_error as go_mod_error, Result};

pub const BUILDKIT_MODULE: &str = "github.com/moby/buildkit";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildkitVersion {
    Tagged(String),
    Pseudo {
        version: String,
        commit_prefix: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildkitRequirement {
    pub version: BuildkitVersion,
}

pub fn parse_buildkit_requirement(contents: &str) -> Result<BuildkitRequirement> {
    parse_module_requirement(contents, BUILDKIT_MODULE)
}

pub fn parse_module_requirement(contents: &str, module: &str) -> Result<BuildkitRequirement> {
    // gomod-parser does not currently accept comment-only lines inside a
    // directive block, although they are valid go.mod syntax. See
    // https://github.com/baz-scm/gomod-parser/issues/45.
    let normalized = contents
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let go_mod = GoMod::from_str(&normalized)
        .map_err(|error| go_mod_error(format!("could not parse go.mod: {error}")))?;

    if go_mod
        .replace
        .iter()
        .any(|replacement| replacement.module_path == module)
    {
        return Err(go_mod_error(format!("go.mod replaces {module}")));
    }
    if go_mod
        .exclude
        .iter()
        .any(|dependency| dependency.module.module_path == module)
    {
        return Err(go_mod_error(format!("go.mod excludes {module}")));
    }

    let requirements: Vec<_> = go_mod
        .require
        .iter()
        .filter(|requirement| requirement.module.module_path == module)
        .collect();
    let [requirement] = requirements.as_slice() else {
        return if requirements.is_empty() {
            Err(go_mod_error(format!(
                "go.mod does not directly require {module}"
            )))
        } else {
            Err(go_mod_error(format!(
                "go.mod contains multiple direct requirements for {module}"
            )))
        };
    };
    if requirement.indirect {
        return Err(go_mod_error(format!(
            "go.mod marks direct requirement {module} as indirect"
        )));
    }

    Ok(BuildkitRequirement {
        version: parse_version(&requirement.module.version)?,
    })
}

pub fn resolve_vtprotobuf_revision<R: Remote>(
    remote: &R,
    buildkit_go_mod: &str,
) -> Result<String> {
    const MODULE: &str = "github.com/planetscale/vtprotobuf";
    let requirement = parse_module_requirement(buildkit_go_mod, MODULE)?;
    match requirement.version {
        BuildkitVersion::Tagged(version) => remote.resolve_tag("planetscale", "vtprotobuf", &version),
        BuildkitVersion::Pseudo { commit_prefix, .. } => {
            remote.resolve_commit_prefix("planetscale", "vtprotobuf", &commit_prefix)
        }
    }
}

fn parse_version(version: &str) -> Result<BuildkitVersion> {
    if let Some((base, _timestamp, commit_prefix)) = pseudo_parts(version) {
        let semver_base = base.strip_prefix('v').unwrap_or(base);
        Version::parse(semver_base).map_err(|error| {
            go_mod_error(format!("malformed BuildKit pseudo-version {version:?}: {error}"))
        })?;
        return Ok(BuildkitVersion::Pseudo {
            version: version.into(),
            commit_prefix: commit_prefix.into(),
        });
    }

    let semver = version.strip_prefix('v').unwrap_or(version);
    Version::parse(semver).map_err(|error| {
        go_mod_error(format!("malformed BuildKit module version {version:?}: {error}"))
    })?;
    Ok(BuildkitVersion::Tagged(version.into()))
}

fn pseudo_parts(version: &str) -> Option<(&str, &str, &str)> {
    let (prefix, commit_prefix) = version.rsplit_once('-')?;
    let (base, timestamp) = prefix.rsplit_once('-')?;
    let timestamp = timestamp.strip_prefix("0.").unwrap_or(timestamp);
    if commit_prefix.len() != 12
        || !commit_prefix.bytes().all(|byte| byte.is_ascii_hexdigit())
        || timestamp.len() != 14
        || !timestamp.bytes().all(|byte| byte.is_ascii_digit())
        || !base.starts_with('v')
    {
        return None;
    }
    Some((base, timestamp, commit_prefix))
}

#[cfg(test)]
mod tests {
    use super::{parse_buildkit_requirement, parse_module_requirement, BuildkitVersion};

    const TAGGED: &str = r#"
module github.com/moby/moby/v2

require (
    github.com/moby/buildkit v0.29.0
    github.com/example/other v1.2.3 // indirect
)
"#;

    #[test]
    fn parses_direct_block_requirement() {
        assert_eq!(
            parse_buildkit_requirement(TAGGED).unwrap().version,
            BuildkitVersion::Tagged("v0.29.0".into())
        );
    }

    #[test]
    fn parses_single_line_requirement() {
        let requirement = parse_buildkit_requirement(
            "module example\nrequire github.com/moby/buildkit v0.29.0\n",
        )
        .unwrap();
        assert_eq!(
            requirement.version,
            BuildkitVersion::Tagged("v0.29.0".into())
        );
    }

    #[test]
    fn parses_pseudo_versions() {
        let requirement = parse_buildkit_requirement(
            "require github.com/moby/buildkit v0.0.0-20251211185533-a2aa163d723f\n",
        )
        .unwrap();
        assert_eq!(
            requirement.version,
            BuildkitVersion::Pseudo {
                version: "v0.0.0-20251211185533-a2aa163d723f".into(),
                commit_prefix: "a2aa163d723f".into(),
            }
        );
    }

    #[test]
    fn parses_tagged_base_pseudo_versions() {
        let requirement = parse_module_requirement(
            "require github.com/planetscale/vtprotobuf v0.6.1-0.20240319094008-0393e58bdf10\n",
            "github.com/planetscale/vtprotobuf",
        )
        .unwrap();
        assert_eq!(
            requirement.version,
            BuildkitVersion::Pseudo {
                version: "v0.6.1-0.20240319094008-0393e58bdf10".into(),
                commit_prefix: "0393e58bdf10".into(),
            }
        );
    }

    #[test]
    fn rejects_indirect_missing_duplicate_and_malformed_requirements() {
        for contents in [
            "require github.com/moby/buildkit v0.29.0 // indirect\n",
            "require github.com/example/other v1.0.0\n",
            "require github.com/moby/buildkit v0.29.0\nrequire github.com/moby/buildkit v0.30.0\n",
            "require github.com/moby/buildkit not-a-version\n",
        ] {
            assert!(parse_buildkit_requirement(contents).is_err(), "{contents}");
        }
    }

    #[test]
    fn rejects_replacements_in_single_lines_and_blocks() {
        let single = "replace github.com/moby/buildkit => ../buildkit\n";
        assert!(parse_buildkit_requirement(&format!("{TAGGED}\n{single}")).is_err());

        let block = "replace (\n github.com/moby/buildkit v0.29.0 => github.com/example/fork v0.1.0\n)\n";
        assert!(parse_buildkit_requirement(&format!("{TAGGED}\n{block}")).is_err());
    }

    #[test]
    fn rejects_exclusions_with_comment_only_lines() {
        let contents = format!(
            "{TAGGED}\nexclude (\n // explanatory comment\n github.com/moby/buildkit v0.28.0\n)\n"
        );
        assert!(parse_buildkit_requirement(&contents).is_err());
    }

    #[test]
    fn rejects_unterminated_blocks() {
        assert!(parse_buildkit_requirement("require (\n github.com/moby/buildkit v0.29.0\n")
            .is_err());
    }
}
