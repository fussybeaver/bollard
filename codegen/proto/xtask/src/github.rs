use std::env;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Deserialize;

use crate::support::{validate_commit as validate_revision, Result};
use crate::support::xtask_error as remote_error;

const API_BASE: &str = "https://api.github.com";
const RAW_BASE: &str = "https://raw.githubusercontent.com";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_RESPONSE_BYTES: u64 = 16 * 1024 * 1024;

pub trait Remote {
    fn resolve_tag(&self, owner: &str, repository: &str, tag: &str) -> Result<String>;
    fn resolve_commit_prefix(
        &self,
        owner: &str,
        repository: &str,
        prefix: &str,
    ) -> Result<String>;
    fn fetch_raw(
        &self,
        owner: &str,
        repository: &str,
        revision: &str,
        path: &str,
    ) -> Result<Vec<u8>>;
}

pub struct GitHubRemote {
    token: Option<String>,
}

impl GitHubRemote {
    pub fn from_environment() -> Self {
        Self {
            token: env::var("GITHUB_TOKEN").ok().filter(|token| !token.is_empty()),
        }
    }

    fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let body = self.get(url, "application/vnd.github+json")?;
        serde_json::from_slice(&body)
            .map_err(|error| remote_error(format!("invalid GitHub response from {url}: {error}")))
    }

    fn get(&self, url: &str, accept: &str) -> Result<Vec<u8>> {
        self.get_with_options(
            url,
            accept,
            CONNECT_TIMEOUT,
            REQUEST_TIMEOUT,
            MAX_RESPONSE_BYTES,
        )
    }

    fn get_with_options(
        &self,
        url: &str,
        accept: &str,
        connect_timeout: Duration,
        request_timeout: Duration,
        max_response_bytes: u64,
    ) -> Result<Vec<u8>> {
        let mut request = ureq::get(url)
            .header("Accept", accept)
            .header("User-Agent", "bollard-buildkit-xtask");
        if let Some(token) = &self.token {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        let mut response = request
            .config()
            .timeout_connect(Some(connect_timeout))
            .timeout_global(Some(request_timeout))
            .timeout_recv_body(Some(request_timeout))
            .build()
            .call()
            .map_err(|error| remote_error(format!("GitHub request failed for {url}: {error}")))?;
        response
            .body_mut()
            .with_config()
            .limit(max_response_bytes)
            .read_to_vec()
            .map_err(|error| remote_error(format!("could not read GitHub response from {url}: {error}")))
    }
}

impl Remote for GitHubRemote {
    fn resolve_tag(&self, owner: &str, repository: &str, tag: &str) -> Result<String> {
        let reference = format!("{API_BASE}/repos/{owner}/{repository}/git/ref/tags/{tag}");
        let tag_ref: GitRef = self.get_json(&reference)?;
        let annotated = if tag_ref.object.r#type == "tag" {
                let tag_object = format!(
                    "{API_BASE}/repos/{owner}/{repository}/git/tags/{}",
                    tag_ref.object.sha
                );
                Some(self.get_json::<GitTag>(&tag_object)?)
            } else {
                None
            };
        resolve_tag_target(&tag_ref, annotated.as_ref(), tag)
    }

    fn resolve_commit_prefix(
        &self,
        owner: &str,
        repository: &str,
        prefix: &str,
    ) -> Result<String> {
        let url = format!("{API_BASE}/repos/{owner}/{repository}/commits/{prefix}");
        let commit: Commit = self.get_json(&url)?;
        let sha = validate_commit(&commit.sha, "commit lookup result")?;
        if !sha.starts_with(prefix) {
            return Err(remote_error(format!(
                "GitHub commit lookup returned {sha}, which does not match {prefix}"
            )));
        }
        Ok(sha)
    }

    fn fetch_raw(
        &self,
        owner: &str,
        repository: &str,
        revision: &str,
        path: &str,
    ) -> Result<Vec<u8>> {
        let url = format!("{RAW_BASE}/{owner}/{repository}/{revision}/{path}");
        self.get(&url, "text/plain")
    }
}

#[derive(Debug, Deserialize)]
struct GitRef {
    object: GitObject,
}

#[derive(Debug, Deserialize)]
struct GitTag {
    object: GitObject,
}

#[derive(Debug, Deserialize)]
struct GitObject {
    sha: String,
    #[serde(rename = "type")]
    r#type: String,
}

#[derive(Debug, Deserialize)]
struct Commit {
    sha: String,
}

fn validate_commit(value: &str, description: &str) -> Result<String> {
    validate_revision(&format!("GitHub {description}"), value)?;
    Ok(value.into())
}

fn resolve_tag_target(
    tag_ref: &GitRef,
    annotated: Option<&GitTag>,
    tag: &str,
) -> Result<String> {
    match tag_ref.object.r#type.as_str() {
        "commit" => validate_commit(&tag_ref.object.sha, "tag target"),
        "tag" => {
            let annotated = annotated.ok_or_else(|| {
                remote_error(format!("GitHub annotated tag {tag:?} has no tag object"))
            })?;
            if annotated.object.r#type != "commit" {
                return Err(remote_error(format!(
                    "GitHub annotated tag {tag:?} resolves to {}, not a commit",
                    annotated.object.r#type
                )));
            }
            validate_commit(&annotated.object.sha, "annotated tag target")
        }
        kind => Err(remote_error(format!(
            "GitHub tag {tag:?} resolves to unsupported object type {kind:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    use super::{resolve_tag_target, GitHubRemote, GitRef, GitTag};

    const COMMIT: &str = "6c91b92cc71077b70c779c510da125301a8e40f3";

    #[test]
    fn resolves_lightweight_tag_objects() {
        let tag_ref: GitRef = serde_json::from_str(&format!(
            r#"{{"object":{{"sha":"{COMMIT}","type":"commit"}}}}"#
        ))
        .unwrap();
        assert_eq!(resolve_tag_target(&tag_ref, None, "v1.0.0").unwrap(), COMMIT);
    }

    #[test]
    fn peels_annotated_tag_objects() {
        let tag_ref: GitRef = serde_json::from_str(
            r#"{"object":{"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","type":"tag"}}"#,
        )
        .unwrap();
        let annotated: GitTag = serde_json::from_str(&format!(
            r#"{{"object":{{"sha":"{COMMIT}","type":"commit"}}}}"#
        ))
        .unwrap();
        assert_eq!(
            resolve_tag_target(&tag_ref, Some(&annotated), "v1.0.0").unwrap(),
            COMMIT
        );
    }

    #[test]
    fn rejects_non_commit_annotated_targets() {
        let tag_ref: GitRef = serde_json::from_str(
            r#"{"object":{"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","type":"tag"}}"#,
        )
        .unwrap();
        let annotated: GitTag = serde_json::from_str(
            r#"{"object":{"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","type":"tree"}}"#,
        )
        .unwrap();
        assert!(resolve_tag_target(&tag_ref, Some(&annotated), "v1.0.0")
            .unwrap_err()
            .to_string()
            .contains("not a commit"));
    }

    fn serve(response: String) -> String {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 1024];
            let _ = stream.read(&mut request);
            stream.write_all(response.as_bytes()).unwrap();
        });
        format!("http://{address}")
    }

    #[test]
    fn limits_response_body_size() {
        let body = "x".repeat(32);
        let url = serve(format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(), body
        ));
        let error = GitHubRemote { token: None }
            .get_with_options(
                &url,
                "text/plain",
                Duration::from_secs(1),
                Duration::from_secs(1),
                8,
            )
            .unwrap_err();
        assert!(error.to_string().contains("could not read GitHub response"));
    }

    #[test]
    fn times_out_stalled_response() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_secs(2));
        });
        let error = GitHubRemote { token: None }
            .get_with_options(
                &format!("http://{address}"),
                "text/plain",
                Duration::from_millis(50),
                Duration::from_millis(50),
                1024,
            )
            .unwrap_err();
        assert!(error.to_string().contains("GitHub request failed"));
    }
}
