//! Options which can be passed to various `Docker` commands.

use url::form_urlencoded;

/// Options for `Docker::containers`.  This uses a "builder" pattern, so
/// most methods will consume the object and return a new one.
#[derive(Debug, Clone, Default)]
pub struct ContainerListOptions {
    all: bool,
    //before: Option<String>,
    //filter: Filter,
    latest: bool,
    limit: Option<u64>,
    //since: Option<String>,
    size: bool,
}

impl ContainerListOptions {
    /// Return all containers, including stopped ones.
    pub fn all(mut self) -> Self {
        self.all = true;
        self
    }

    /// Return just the most-recently-started container (even if it has
    /// stopped).
    pub fn latest(mut self) -> Self {
        self.latest = true;
        self
    }

    /// Limit the number of containers we return.
    pub fn limit(mut self, n: u64) -> Self {
        self.limit = Some(n);
        self
    }

    /// Calculate the total file sizes for our containers.  **WARNING:**
    /// This is very expensive.
    pub fn size(mut self) -> Self {
        self.size = true;
        self
    }

    /// Convert to URL parameters.
    pub fn to_url_params(&self) -> String {
        let mut params = form_urlencoded::Serializer::new(String::new());
        if self.all {
            params.append_pair("all", "1");
        }
        if self.latest {
            params.append_pair("latest", "1");
        }
        if let Some(limit) = self.limit {
            params.append_pair("limit", &limit.to_string());
        }
        if self.size {
            params.append_pair("size", "1");
        }
        params.finish()
    }
}
