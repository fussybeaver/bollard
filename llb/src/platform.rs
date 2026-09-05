//! Platform constraints for LLB operations.
//!
//! Mirrors Go's `github.com/moby/buildkit/client/llb.Platform` and the
//! well-known constants such as `llb.LinuxAmd64`.

use std::borrow::Cow;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use bollard_buildkit_proto::pb;

use crate::error::LlbError;

/// An OCI platform specification.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Platform {
    /// Operating system (e.g. `linux`, `darwin`, `windows`).
    pub os: Cow<'static, str>,
    /// CPU architecture (e.g. `amd64`, `arm64`).
    pub architecture: Cow<'static, str>,
    /// Optional CPU variant (e.g. `v7`, `v8`).
    pub variant: Option<Cow<'static, str>>,
    /// Optional OS version.
    pub os_version: Option<Cow<'static, str>>,
    /// Optional OS features.
    pub os_features: Vec<String>,
}

impl Platform {
    /// Create a new platform with the given OS and architecture.
    pub fn new<OS: Into<String>, ARCH: Into<String>>(os: OS, architecture: ARCH) -> Self {
        let os = normalize_os(&os.into());
        let (architecture, variant) = normalize_architecture(&architecture.into(), None);
        Self {
            os: Cow::Owned(os),
            architecture: Cow::Owned(architecture),
            variant: variant.map(Cow::Owned),
            os_version: None,
            os_features: Vec::new(),
        }
    }

    /// Set the CPU variant.
    pub fn with_variant<S: Into<String>>(mut self, variant: S) -> Self {
        let (architecture, variant) =
            normalize_architecture(self.architecture.as_ref(), Some(variant.into().as_str()));
        self.architecture = Cow::Owned(architecture);
        self.variant = variant.map(Cow::Owned);
        self
    }

    /// Set the OS version.
    pub fn with_os_version<S: Into<String>>(mut self, version: S) -> Self {
        self.os_version = Some(Cow::Owned(version.into()));
        self
    }

    /// Add an OS feature.
    pub fn with_os_feature<S: Into<String>>(mut self, feature: S) -> Self {
        self.os_features.push(feature.into());
        self
    }

    /// `linux/amd64`
    pub const LINUX_AMD64: Platform = Platform {
        os: Cow::Borrowed("linux"),
        architecture: Cow::Borrowed("amd64"),
        variant: None,
        os_version: None,
        os_features: Vec::new(),
    };

    /// `linux/arm64`
    pub const LINUX_ARM64: Platform = Platform {
        os: Cow::Borrowed("linux"),
        architecture: Cow::Borrowed("arm64"),
        variant: None,
        os_version: None,
        os_features: Vec::new(),
    };

    /// `linux/arm/v7`
    pub const LINUX_ARM_V7: Platform = Platform {
        os: Cow::Borrowed("linux"),
        architecture: Cow::Borrowed("arm"),
        variant: Some(Cow::Borrowed("v7")),
        os_version: None,
        os_features: Vec::new(),
    };

    /// `linux/386`
    pub const LINUX_386: Platform = Platform {
        os: Cow::Borrowed("linux"),
        architecture: Cow::Borrowed("386"),
        variant: None,
        os_version: None,
        os_features: Vec::new(),
    };

    /// `darwin/arm64`
    pub const DARWIN_ARM64: Platform = Platform {
        os: Cow::Borrowed("darwin"),
        architecture: Cow::Borrowed("arm64"),
        variant: None,
        os_version: None,
        os_features: Vec::new(),
    };

    /// `windows/amd64`
    pub const WINDOWS_AMD64: Platform = Platform {
        os: Cow::Borrowed("windows"),
        architecture: Cow::Borrowed("amd64"),
        variant: None,
        os_version: None,
        os_features: Vec::new(),
    };
}

fn normalize_os(os: &str) -> String {
    match os.to_ascii_lowercase().as_str() {
        "macos" => "darwin".to_string(),
        normalized => normalized.to_string(),
    }
}

fn normalize_architecture(architecture: &str, variant: Option<&str>) -> (String, Option<String>) {
    let mut architecture = architecture.to_ascii_lowercase();
    let mut variant = variant.map(str::to_ascii_lowercase);

    match architecture.as_str() {
        "i386" => {
            architecture = "386".to_string();
            variant = None;
        }
        "x86_64" | "x86-64" | "amd64" => {
            architecture = "amd64".to_string();
            if variant.as_deref() == Some("v1") {
                variant = None;
            }
        }
        "aarch64" | "arm64" => {
            architecture = "arm64".to_string();
            if matches!(variant.as_deref(), Some("8" | "v8" | "v8.0")) {
                variant = None;
            } else if matches!(variant.as_deref(), Some("9" | "9.0" | "v9.0")) {
                variant = Some("v9".to_string());
            }
        }
        "armhf" => {
            architecture = "arm".to_string();
            variant = Some("v7".to_string());
        }
        "armel" => {
            architecture = "arm".to_string();
            variant = Some("v6".to_string());
        }
        "arm" => match variant.as_deref() {
            None | Some("") | Some("7") => variant = Some("v7".to_string()),
            Some("5") | Some("6") | Some("8") => {
                variant = Some(format!("v{}", variant.as_deref().unwrap()))
            }
            _ => {}
        },
        _ => {}
    }

    (architecture, variant)
}

fn valid_component(component: &str) -> bool {
    !component.is_empty()
        && component
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
}

impl Default for Platform {
    fn default() -> Self {
        Self::LINUX_AMD64.clone()
    }
}

impl Display for Platform {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.os.as_ref(), self.architecture.as_ref())?;
        if let Some(variant) = &self.variant {
            write!(f, "/{}", variant.as_ref())?;
        }
        Ok(())
    }
}

impl FromStr for Platform {
    type Err = LlbError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('/').collect();
        match parts.len() {
            2 if valid_component(parts[0]) && valid_component(parts[1]) => {
                Ok(Platform::new(parts[0], parts[1]))
            }
            3 if valid_component(parts[0])
                && valid_component(parts[1])
                && valid_component(parts[2]) =>
            {
                Ok(Platform::new(parts[0], parts[1]).with_variant(parts[2]))
            }
            _ => Err(LlbError::InvalidReference {
                reference: s.to_string(),
            }),
        }
    }
}

impl From<Platform> for pb::Platform {
    fn from(p: Platform) -> Self {
        let os = normalize_os(p.os.as_ref());
        let (architecture, variant) =
            normalize_architecture(p.architecture.as_ref(), p.variant.as_deref());
        Self {
            architecture,
            os,
            variant: variant.unwrap_or_default(),
            os_version: p.os_version.map(Cow::into_owned).unwrap_or_default(),
            os_features: p.os_features,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_round_trip() {
        let p: Platform = "linux/arm64".parse().unwrap();
        assert_eq!(p.os, "linux");
        assert_eq!(p.architecture, "arm64");
        assert_eq!(p.variant, None);

        let p2: Platform = "linux/arm/v7".parse().unwrap();
        assert_eq!(p2.to_string(), "linux/arm/v7");
    }

    #[test]
    fn parse_invalid_fails() {
        assert!("linux".parse::<Platform>().is_err());
        assert!("linux/amd64/extra/junk".parse::<Platform>().is_err());
        assert!("/amd64".parse::<Platform>().is_err());
        assert!("linux/".parse::<Platform>().is_err());
        assert!("linux//v7".parse::<Platform>().is_err());
        assert!("linux/amd64/".parse::<Platform>().is_err());
    }

    #[test]
    fn parse_normalizes_containerd_aliases() {
        assert_eq!(
            "Linux/X86_64".parse::<Platform>().unwrap().to_string(),
            "linux/amd64"
        );
        assert_eq!(
            "linux/aarch64".parse::<Platform>().unwrap().to_string(),
            "linux/arm64"
        );
        assert_eq!(
            "linux/armhf".parse::<Platform>().unwrap().to_string(),
            "linux/arm/v7"
        );
        assert_eq!(
            "macos/amd64".parse::<Platform>().unwrap().to_string(),
            "darwin/amd64"
        );
    }

    #[test]
    fn constants_are_populated() {
        assert_eq!(Platform::LINUX_AMD64.to_string(), "linux/amd64");
        assert_eq!(Platform::LINUX_ARM64.to_string(), "linux/arm64");
        assert_eq!(Platform::LINUX_ARM_V7.to_string(), "linux/arm/v7");
        assert_eq!(Platform::LINUX_386.to_string(), "linux/386");
        assert_eq!(Platform::DARWIN_ARM64.to_string(), "darwin/arm64");
        assert_eq!(Platform::WINDOWS_AMD64.to_string(), "windows/amd64");
    }
}
