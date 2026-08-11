use std::error::Error;
use std::fmt::{Display, Formatter};

use semver::Version;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

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

#[derive(Debug)]
struct GoModError(String);

impl Display for GoModError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for GoModError {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Block {
    Require,
    Replace,
    Exclude,
    Retract,
}

pub fn parse_buildkit_requirement(contents: &str) -> Result<BuildkitRequirement> {
    let mut block = None;
    let mut requirements = Vec::new();

    for raw_line in contents.lines() {
        let (code, comment) = raw_line.split_once("//").unwrap_or((raw_line, ""));
        let line = code.trim();
        let starts_block = line.ends_with('(');

        if let Some(active_block) = block {
            if line == ")" {
                block = None;
                continue;
            }
            if active_block == Block::Require {
                parse_requirement_line(line, comment, &mut requirements)?;
            } else if matches!(active_block, Block::Replace | Block::Exclude | Block::Retract)
                && line.contains(BUILDKIT_MODULE)
            {
                return Err(go_mod_error(format!(
                    "Moby go.mod contains a {active_block} directive for {BUILDKIT_MODULE}"
                )));
            }
            continue;
        }

        if starts_block {
            block = match line {
                "require (" => Some(Block::Require),
                "replace (" => Some(Block::Replace),
                "exclude (" => Some(Block::Exclude),
                "retract (" => Some(Block::Retract),
                _ => None,
            };
            continue;
        }

        if let Some(rest) = line.strip_prefix("require ") {
            parse_requirement_line(rest, comment, &mut requirements)?;
        } else if let Some(rest) = line.strip_prefix("replace ") {
            if replacement_targets_buildkit(rest) {
                return Err(go_mod_error(format!(
                    "Moby go.mod replaces {BUILDKIT_MODULE}"
                )));
            }
        } else if (line.starts_with("exclude ") || line.starts_with("retract "))
            && line.contains(BUILDKIT_MODULE)
        {
            return Err(go_mod_error(format!(
                "Moby go.mod excludes or retracts {BUILDKIT_MODULE}"
            )));
        }
    }

    if block.is_some() {
        return Err(go_mod_error("Moby go.mod contains an unterminated block"));
    }
    match requirements.as_slice() {
        [] => Err(go_mod_error(format!(
            "Moby go.mod does not directly require {BUILDKIT_MODULE}"
        ))),
        [requirement] => Ok(requirement.clone()),
        _ => Err(go_mod_error(format!(
            "Moby go.mod contains multiple direct requirements for {BUILDKIT_MODULE}"
        ))),
    }
}

fn parse_requirement_line(
    line: &str,
    comment: &str,
    requirements: &mut Vec<BuildkitRequirement>,
) -> Result<()> {
    let mut fields = line.split_whitespace();
    let Some(module) = fields.next() else {
        return Ok(());
    };
    if module != BUILDKIT_MODULE {
        return Ok(());
    }
    if comment.contains("indirect") {
        return Err(go_mod_error(format!(
            "Moby go.mod marks direct requirement {BUILDKIT_MODULE} as indirect"
        )));
    }
    let Some(version) = fields.next() else {
        return Err(go_mod_error(format!(
            "Moby go.mod has no version for {BUILDKIT_MODULE}"
        )));
    };
    if fields.next().is_some() {
        return Err(go_mod_error(format!(
            "Moby go.mod has malformed requirement for {BUILDKIT_MODULE}"
        )));
    }
    requirements.push(BuildkitRequirement {
        version: parse_version(version)?,
    });
    Ok(())
}

fn replacement_targets_buildkit(line: &str) -> bool {
    let target = line.split_once("=>").map_or(line, |(target, _)| target);
    target.split_whitespace().next() == Some(BUILDKIT_MODULE)
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
    let mut parts = version.rsplitn(3, '-');
    let commit_prefix = parts.next()?;
    let timestamp = parts.next()?;
    let base = parts.next()?;
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

fn go_mod_error(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(GoModError(message.into()))
}

impl Display for Block {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Require => "require",
            Self::Replace => "replace",
            Self::Exclude => "exclude",
            Self::Retract => "retract",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_buildkit_requirement, BuildkitVersion};

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
    fn rejects_unterminated_blocks() {
        assert!(parse_buildkit_requirement("require (\n github.com/moby/buildkit v0.29.0\n")
            .unwrap_err()
            .to_string()
            .contains("unterminated"));
    }
}
