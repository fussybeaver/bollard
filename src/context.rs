//! Docker CLI context resolution.
//!
//! Resolves the Docker daemon host using the same precedence as the Docker CLI:
//!
//! 1. The `DOCKER_HOST` environment variable.
//! 2. The context named by `DOCKER_CONTEXT`.
//! 3. The `currentContext` field of `~/.docker/config.json`.
//! 4. The platform-specific default ([`DEFAULT_DOCKER_HOST`](crate::DEFAULT_DOCKER_HOST)).
//!
//! Steps 2 and 3 share the same context lookup: the name `default` (or an empty
//! value) means the platform default; any other name is looked up under
//! `$DOCKER_CONFIG/contexts/meta/<dir>/meta.json` (defaulting to `~/.docker`).
//! A name that does not resolve produces [`Error::DockerContextNotFoundError`].

use std::env;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

use crate::errors::Error;

const DEFAULT_CONTEXT: &str = "default";

/// Locate the Docker config directory: `$DOCKER_CONFIG` if set, else `$HOME/.docker`
/// (or `%USERPROFILE%\.docker` on Windows).
fn docker_config_dir() -> Option<PathBuf> {
    if let Some(dir) = env::var_os("DOCKER_CONFIG") {
        if !dir.is_empty() {
            return Some(PathBuf::from(dir));
        }
    }
    home_dir().map(|p| p.join(".docker"))
}

#[cfg(unix)]
fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

#[cfg(windows)]
fn home_dir() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

#[derive(Deserialize)]
struct DockerConfig {
    #[serde(rename = "currentContext", default)]
    current_context: Option<String>,
}

#[derive(Deserialize)]
struct ContextMeta {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Endpoints", default)]
    endpoints: ContextEndpoints,
}

#[derive(Deserialize, Default)]
struct ContextEndpoints {
    #[serde(default)]
    docker: Option<ContextEndpoint>,
}

#[derive(Deserialize)]
struct ContextEndpoint {
    #[serde(rename = "Host", default)]
    host: Option<String>,
}

/// Read the `currentContext` field from `~/.docker/config.json`. Returns `None` if
/// the file is missing, unparseable, or has no `currentContext` value.
fn current_context_from_config() -> Option<String> {
    let path = docker_config_dir()?.join("config.json");
    let contents = fs::read_to_string(path).ok()?;
    let cfg: DockerConfig = serde_json::from_str(&contents).ok()?;
    cfg.current_context.filter(|s| !s.is_empty())
}

/// Look up a context by name and return its `Endpoints.docker.Host`. Returns
/// `Ok(None)` if the context cannot be found, mirroring how Docker stores
/// context metadata as JSON files indexed by SHA-256 of the context name.
///
/// The lookup iterates the metadata directory rather than recomputing the
/// hash, which avoids a SHA-256 dependency. The number of stored contexts is
/// typically small.
fn lookup_context_host(name: &str) -> Option<String> {
    let dir = docker_config_dir()?.join("contexts").join("meta");
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let meta_path = entry.path().join("meta.json");
        let contents = match fs::read_to_string(&meta_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let meta: ContextMeta = match serde_json::from_str(&contents) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.name == name {
            return meta.endpoints.docker.and_then(|d| d.host);
        }
    }
    None
}

/// Resolve a context name to its Docker endpoint host.
///
/// The name `"default"` (or an empty string) returns the supplied
/// `default_host`. Any other name is looked up under
/// `$DOCKER_CONFIG/contexts/meta/*/meta.json`.
///
/// # Errors
///
/// Returns [`Error::DockerContextNotFoundError`] if `name` is not `"default"`
/// and no matching context is found.
pub fn host_for_context(name: &str, default_host: &str) -> Result<String, Error> {
    if name.is_empty() || name == DEFAULT_CONTEXT {
        return Ok(default_host.to_string());
    }
    lookup_context_host(name).ok_or_else(|| Error::DockerContextNotFoundError {
        name: name.to_string(),
    })
}

/// Return the name of the "current" Docker context, considering only the
/// Docker CLI's context-selection inputs:
///
/// 1. The `DOCKER_CONTEXT` environment variable, if non-empty.
/// 2. The `currentContext` field of `$DOCKER_CONFIG/config.json`, if present.
///
/// Returns `None` if neither is set (the default context is implicit and is
/// not represented by a name on disk). Notably this does **not** look at
/// `DOCKER_HOST`, which is a transport override rather than a context name.
pub fn current_context_name() -> Option<String> {
    env::var("DOCKER_CONTEXT")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(current_context_from_config)
}

/// Resolve the Docker daemon host using the Docker CLI precedence rules.
///
/// See the [module-level documentation](crate::context) for the full ordering.
///
/// # Errors
///
/// Returns [`Error::DockerContextNotFoundError`] if `DOCKER_CONTEXT` or the
/// config file's `currentContext` names a context that does not exist on disk.
pub fn resolve_host(default_host: &str) -> Result<String, Error> {
    if let Some(host) = env::var("DOCKER_HOST").ok().filter(|s| !s.is_empty()) {
        return Ok(host);
    }
    match current_context_name() {
        Some(name) => host_for_context(&name, default_host),
        None => Ok(default_host.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_lock::ENV_LOCK;

    /// RAII guard that sets an env var and restores the previous value on drop.
    struct TempEnvVar {
        key: String,
        prev: Option<String>,
    }

    impl TempEnvVar {
        fn set(key: &str, val: &str) -> Self {
            let prev = env::var(key).ok();
            env::set_var(key, val);
            Self {
                key: key.to_string(),
                prev,
            }
        }

        fn unset(key: &str) -> Self {
            let prev = env::var(key).ok();
            env::remove_var(key);
            Self {
                key: key.to_string(),
                prev,
            }
        }
    }

    impl Drop for TempEnvVar {
        fn drop(&mut self) {
            match &self.prev {
                Some(v) => env::set_var(&self.key, v),
                None => env::remove_var(&self.key),
            }
        }
    }

    fn write_meta(dir: &std::path::Path, sub: &str, name: &str, host: &str) {
        let meta_dir = dir.join("contexts").join("meta").join(sub);
        fs::create_dir_all(&meta_dir).unwrap();
        let body = format!(r#"{{"Name":"{name}","Endpoints":{{"docker":{{"Host":"{host}"}}}}}}"#);
        fs::write(meta_dir.join("meta.json"), body).unwrap();
    }

    fn write_config(dir: &std::path::Path, current: Option<&str>) {
        let body = match current {
            Some(c) => format!(r#"{{"currentContext":"{c}"}}"#),
            None => "{}".to_string(),
        };
        fs::write(dir.join("config.json"), body).unwrap();
    }

    #[test]
    fn docker_host_wins_over_everything() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        write_config(tmp.path(), Some("some-context"));
        write_meta(tmp.path(), "aa", "some-context", "tcp://from-context:1234");

        let _cfg = TempEnvVar::set("DOCKER_CONFIG", tmp.path().to_str().unwrap());
        let _ctx = TempEnvVar::set("DOCKER_CONTEXT", "some-context");
        let _host = TempEnvVar::set("DOCKER_HOST", "tcp://from-env:9999");

        let resolved = resolve_host("unix:///default.sock").unwrap();
        assert_eq!(resolved, "tcp://from-env:9999");
    }

    #[test]
    fn docker_context_env_resolves_to_stored_host() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        write_meta(tmp.path(), "abc", "remote", "ssh://user@host");

        let _cfg = TempEnvVar::set("DOCKER_CONFIG", tmp.path().to_str().unwrap());
        let _ctx = TempEnvVar::set("DOCKER_CONTEXT", "remote");
        let _host = TempEnvVar::unset("DOCKER_HOST");

        let resolved = resolve_host("unix:///default.sock").unwrap();
        assert_eq!(resolved, "ssh://user@host");
    }

    #[test]
    fn current_context_from_config_used_when_env_unset() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        write_config(tmp.path(), Some("staging"));
        write_meta(tmp.path(), "xyz", "staging", "tcp://staging:2375");

        let _cfg = TempEnvVar::set("DOCKER_CONFIG", tmp.path().to_str().unwrap());
        let _ctx = TempEnvVar::unset("DOCKER_CONTEXT");
        let _host = TempEnvVar::unset("DOCKER_HOST");

        let resolved = resolve_host("unix:///default.sock").unwrap();
        assert_eq!(resolved, "tcp://staging:2375");
    }

    #[test]
    fn docker_context_overrides_config_current_context() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        write_config(tmp.path(), Some("from-config"));
        write_meta(tmp.path(), "a", "from-config", "tcp://from-config:1");
        write_meta(tmp.path(), "b", "from-env", "tcp://from-env-ctx:2");

        let _cfg = TempEnvVar::set("DOCKER_CONFIG", tmp.path().to_str().unwrap());
        let _ctx = TempEnvVar::set("DOCKER_CONTEXT", "from-env");
        let _host = TempEnvVar::unset("DOCKER_HOST");

        let resolved = resolve_host("unix:///default.sock").unwrap();
        assert_eq!(resolved, "tcp://from-env-ctx:2");
    }

    #[test]
    fn default_context_falls_back_to_default_host() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        write_config(tmp.path(), Some("default"));

        let _cfg = TempEnvVar::set("DOCKER_CONFIG", tmp.path().to_str().unwrap());
        let _ctx = TempEnvVar::unset("DOCKER_CONTEXT");
        let _host = TempEnvVar::unset("DOCKER_HOST");

        let resolved = resolve_host("unix:///default.sock").unwrap();
        assert_eq!(resolved, "unix:///default.sock");
    }

    #[test]
    fn missing_config_and_env_falls_back_to_default() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();

        let _cfg = TempEnvVar::set("DOCKER_CONFIG", tmp.path().to_str().unwrap());
        let _ctx = TempEnvVar::unset("DOCKER_CONTEXT");
        let _host = TempEnvVar::unset("DOCKER_HOST");

        let resolved = resolve_host("unix:///default.sock").unwrap();
        assert_eq!(resolved, "unix:///default.sock");
    }

    #[test]
    fn unknown_context_errors() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();

        let _cfg = TempEnvVar::set("DOCKER_CONFIG", tmp.path().to_str().unwrap());
        let _ctx = TempEnvVar::set("DOCKER_CONTEXT", "missing");
        let _host = TempEnvVar::unset("DOCKER_HOST");

        let err = resolve_host("unix:///default.sock").unwrap_err();
        match err {
            Error::DockerContextNotFoundError { name } => assert_eq!(name, "missing"),
            other => panic!("expected DockerContextNotFoundError, got {other:?}"),
        }
    }

    #[test]
    fn host_for_context_resolves_named() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        write_meta(tmp.path(), "x", "prod", "tcp://prod:2375");
        let _cfg = TempEnvVar::set("DOCKER_CONFIG", tmp.path().to_str().unwrap());

        assert_eq!(
            host_for_context("prod", "unix:///default.sock").unwrap(),
            "tcp://prod:2375"
        );
    }

    #[test]
    fn host_for_context_default_returns_default() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let _cfg = TempEnvVar::set("DOCKER_CONFIG", tmp.path().to_str().unwrap());

        assert_eq!(
            host_for_context("default", "unix:///default.sock").unwrap(),
            "unix:///default.sock"
        );
        assert_eq!(
            host_for_context("", "unix:///default.sock").unwrap(),
            "unix:///default.sock"
        );
    }

    #[test]
    fn host_for_context_unknown_errors() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let _cfg = TempEnvVar::set("DOCKER_CONFIG", tmp.path().to_str().unwrap());

        let err = host_for_context("ghost", "unix:///default.sock").unwrap_err();
        match err {
            Error::DockerContextNotFoundError { name } => assert_eq!(name, "ghost"),
            other => panic!("expected DockerContextNotFoundError, got {other:?}"),
        }
    }

    #[test]
    fn current_context_name_reads_env_then_config() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        write_config(tmp.path(), Some("from-config"));

        let _cfg = TempEnvVar::set("DOCKER_CONFIG", tmp.path().to_str().unwrap());
        let _ctx = TempEnvVar::set("DOCKER_CONTEXT", "from-env");
        let _host = TempEnvVar::unset("DOCKER_HOST");
        assert_eq!(current_context_name().as_deref(), Some("from-env"));

        // env unset → falls back to config
        let _ctx2 = TempEnvVar::unset("DOCKER_CONTEXT");
        assert_eq!(current_context_name().as_deref(), Some("from-config"));
    }

    #[test]
    fn current_context_name_ignores_docker_host() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let _cfg = TempEnvVar::set("DOCKER_CONFIG", tmp.path().to_str().unwrap());
        let _ctx = TempEnvVar::unset("DOCKER_CONTEXT");
        let _host = TempEnvVar::set("DOCKER_HOST", "tcp://override:2375");

        assert_eq!(current_context_name(), None);
    }

    #[test]
    fn empty_docker_host_treated_as_unset() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        write_meta(tmp.path(), "abc", "remote", "tcp://remote:2375");

        let _cfg = TempEnvVar::set("DOCKER_CONFIG", tmp.path().to_str().unwrap());
        let _ctx = TempEnvVar::set("DOCKER_CONTEXT", "remote");
        let _host = TempEnvVar::set("DOCKER_HOST", "");

        let resolved = resolve_host("unix:///default.sock").unwrap();
        assert_eq!(resolved, "tcp://remote:2375");
    }
}
