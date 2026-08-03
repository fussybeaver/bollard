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
        Self {
            os: Cow::Owned(os.into()),
            architecture: Cow::Owned(architecture.into()),
            variant: None,
            os_version: None,
            os_features: Vec::new(),
        }
    }

    /// Set the CPU variant.
    pub fn with_variant<S: Into<String>>(mut self, variant: S) -> Self {
        self.variant = Some(Cow::Owned(variant.into()));
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
            2 => Ok(Platform::new(parts[0], parts[1])),
            3 => Ok(Platform::new(parts[0], parts[1]).with_variant(parts[2])),
            _ => Err(LlbError::InvalidReference {
                reference: s.to_string(),
            }),
        }
    }
}

impl From<Platform> for pb::Platform {
    fn from(p: Platform) -> Self {
        Self {
            architecture: p.architecture.into_owned(),
            os: p.os.into_owned(),
            variant: p.variant.map(Cow::into_owned).unwrap_or_default(),
            os_version: p.os_version.map(Cow::into_owned).unwrap_or_default(),
            os_features: p.os_features,
        }
    }
}
