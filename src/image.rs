use chrono::serde::ts_seconds;
use chrono::{DateTime, Utc};
use failure::Error;
use futures::Stream;
use hyper::client::connect::Connect;
use hyper::rt::Future;
use hyper::Method;
use serde_json;

use super::Docker;
use container::Config;
use options::{EncodableQueryString, NoParams};

use std::borrow::Borrow;
use std::collections::HashMap;

/// ## Image
///
/// Image type returned by the [Inspect Image API](../struct.Docker.html#method.inspect_image)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct Image {
    #[serde(rename = "Id")]
    pub id: String,
    pub container: String,
    pub comment: String,
    pub os: String,
    pub architecture: String,
    pub container_config: Config,
    pub parent: String,
    pub created: DateTime<Utc>,
    pub repo_tags: Vec<String>,
    pub size: u64,
    pub virtual_size: u64,
    pub author: String,
}

/// ## APIImages
///
/// APIImages type returned by the [List Images API](../struct.Docker.html#method.list_images)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct APIImages {
    pub id: String,
    pub repo_tags: Option<Vec<String>>,
    #[serde(with = "ts_seconds")]
    pub created: DateTime<Utc>,
    pub size: u64,
    pub virtual_size: u64,
    pub parent_id: String,
    pub repo_digests: Option<Vec<String>>,
    pub labels: Option<HashMap<String, String>>,
}

/// ## Create Image Options
///
/// Parameters available for pulling an image, used in the [Create Image
/// API](../struct.Docker.html#method.create_image)
#[derive(Debug, Clone, Default)]
pub struct CreateImageOptions<T>
where
    T: AsRef<str>,
{
    pub from_image: T,
    pub from_src: T,
    pub repo: T,
    pub tag: T,
    pub platform: T,
}

pub trait CreateImageQueryParams<K, V>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    fn into_array(self) -> Result<Vec<(K, V)>, Error>;
}

impl<'a> CreateImageQueryParams<&'a str, &'a str> for CreateImageOptions<&'a str> {
    fn into_array(self) -> Result<Vec<(&'a str, &'a str)>, Error> {
        Ok(vec![
            ("fromImage", self.from_image),
            ("fromSrc", self.from_src),
            ("repo", self.repo),
            ("tag", self.tag),
            ("platform", self.platform),
        ])
    }
}

impl<'a> CreateImageQueryParams<&'a str, String> for CreateImageOptions<String> {
    fn into_array(self) -> Result<Vec<(&'a str, String)>, Error> {
        Ok(vec![
            ("fromImage", self.from_image),
            ("fromSrc", self.from_src),
            ("repo", self.repo),
            ("tag", self.tag),
            ("platform", self.platform),
        ])
    }
}

/*
impl<'a> CreateImageOptionsTrait<'a> for CreateImage<'a> {
    fn into_array(self) -> Result<Vec<(&'a str, &'a str)>, Error> {
        Ok(vec![
            ("fromImage", self.from_image),
            ("fromSrc", self.from_src),
            ("repo", self.repo),
            ("tag", self.tag),
            ("platform", self.platform),
        ])
    }
}
*/

/*
impl EncodableQueryString for CreateImageOptions {
    fn into_array<'a>(self) -> Result<Vec<(&'a str, String)>, Error> {
        Ok(vec![
            ("fromImage", self.from_image),
            ("fromSrc", self.from_src),
            ("repo", self.repo),
            ("tag", self.tag),
            ("platform", self.platform),
        ])
    }
}*/

/// ## Create Images Results : Create Image Progress Detail
///
/// Subtype for the [Create Image Results](struct.CreateImagesResults.html) type.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateImageProgressDetail {
    pub current: u64,
    pub total: u64,
}

/// ## Create Images Results : Create Image Progress Detail
///
/// Subtype for the [Create Image Results](struct.CreateImagesResults.html) type.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateImageErrorDetail {
    message: String,
}

/// ## Create Image Results
///
/// Result type for the [Create Image API](../struct.Docker.html#method.create_image)
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum CreateImageResults {
    #[serde(rename_all = "camelCase")]
    CreateImageProgressResponse {
        status: String,
        progress_detail: Option<CreateImageProgressDetail>,
        id: Option<String>,
        progress: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    CreateImageError {
        error_detail: CreateImageErrorDetail,
        error: String,
    },
}
/*


/// ## List Images Options
///
/// Parameters to the [List Images
/// API](../struct.Docker.html#method.list_images)
#[derive(Debug, Clone, Default)]
pub struct ListImagesOptions {
    pub all: bool,
    pub filters: HashMap<String, String>,
    pub digests: bool,
}

impl EncodableQueryString for ListImagesOptions {
    fn into_array<'a>(self) -> Result<Vec<(&'a str, String)>, Error> {
        Ok(vec![
            ("all", self.all.to_string()),
            ("filters", serde_json::to_string(&self.filters)?),
            ("digests", self.digests.to_string()),
        ])
    }
}

/// ## Prune Images Options
///
/// Parameters to the [Prune Images API](../struct.Docker.html#method.prune_images)
#[derive(Debug, Clone, Default)]
pub struct PruneImagesOptions {
    pub filters: HashMap<String, String>,
}

/// ## Prune Images Results : Images Deleted
///
/// Subtype for the [Prune Image Results](struct.PruneImagesResults.html) type.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct PruneImagesImagesDeleted {
    pub untagged: Option<String>,
    pub deleted: String,
}

/// ## Prune Images Results
///
/// Result type for the [Prune Images API](../struct.Docker.html#method.prune_images)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct PruneImagesResults {
    pub images_deleted: Option<Vec<PruneImagesImagesDeleted>>,
    pub space_reclaimed: u64,
}

impl EncodableQueryString for PruneImagesOptions {
    fn into_array<'a>(self) -> Result<Vec<(&'a str, String)>, Error> {
        Ok(vec![("filters", serde_json::to_string(&self.filters)?)])
    }
}

/// ## Image History
///
/// Result type for the [Image History API](../struct.Docker.html#method.image_history)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct ImageHistory {
    pub id: String,
    #[serde(with = "ts_seconds")]
    pub created: DateTime<Utc>,
    pub created_by: String,
    pub tags: Option<Vec<String>>,
    pub size: u64,
    pub comment: String,
}

/// ## Search Images Options
///
/// Parameters to the [Search Images API](../struct.Docker.html#method.search_images)
#[derive(Debug, Clone, Default)]
pub struct SearchImagesOptions {
    pub term: String,
    pub limit: Option<u64>,
    pub filters: HashMap<String, String>,
}

impl EncodableQueryString for SearchImagesOptions {
    fn into_array<'a>(self) -> Result<Vec<(&'a str, String)>, Error> {
        Ok(vec![
            ("term", self.term),
            (
                "limit",
                self.limit
                    .map(|limit| limit.to_string())
                    .unwrap_or_else(String::new),
            ),
            ("filters", serde_json::to_string(&self.filters)?),
        ])
    }
}

///
/// ## Image Search Results
///
/// Result type for the [Image Search API](../struct.Docker.html#method.image_search)
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct APIImageSearch {
    pub description: String,
    pub is_official: bool,
    pub is_automated: bool,
    pub name: String,
    pub star_count: u64,
}

/// ## Remove Image Options
///
/// Parameters to the [Remove Image API](../struct.Docker.html#method.remove_image)
#[derive(Debug, Clone, Default)]
pub struct RemoveImageOptions {
    pub force: bool,
    pub noprune: bool,
}

impl EncodableQueryString for RemoveImageOptions {
    fn into_array<'a>(self) -> Result<Vec<(&'a str, String)>, Error> {
        Ok(vec![
            ("force", self.force.to_string()),
            ("noprune", self.noprune.to_string()),
        ])
    }
}

/// ## RemoveImageResults
///
/// Result type for the [Remove Image API](../struct.Docker.html#method.remove_image)
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum RemoveImageResults {
    #[serde(rename_all = "PascalCase")]
    RemoveImageUntagged { untagged: String },
    #[serde(rename_all = "PascalCase")]
    RemoveImageDeleted { deleted: String },
}

/// ## Tag Image Options
///
/// Parameters to the [Tag Image API](../struct.Docker.html#method.tag_image)
#[derive(Debug, Clone, Default)]
pub struct TagImageOptions {
    pub repo: String,
    pub tag: String,
}

impl EncodableQueryString for TagImageOptions {
    fn into_array<'a>(self) -> Result<Vec<(&'a str, String)>, Error> {
        Ok(vec![
            ("repo", self.repo),
            //("tag", self.tag),
        ])
    }
}

/// ## Push Image Options
///
/// Parameters to the [Push Image API](../struct.Docker.html#method.push_image)
#[derive(Debug, Clone, Default)]
pub struct PushImageOptions {
    pub tag: String,
}

impl EncodableQueryString for PushImageOptions {
    fn into_array<'a>(self) -> Result<Vec<(&'a str, String)>, Error> {
        Ok(vec![("tag", self.tag)])
    }
}

*/

impl<C> Docker<C>
where
    C: Connect + Sync + 'static,
{
    /*
    /// ---
    ///
    /// # List Images
    ///
    /// Returns a list of images on the server. Note that it uses a different, smaller
    /// representation of an image than inspecting a single image
    ///
    ///  # Returns
    ///
    ///  - Vector of [APIImages](image/struct.APIImages.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_unix_defaults().unwrap();
    /// use boondock::image::ListImagesOptions;
    ///
    /// use std::default::Default;
    ///
    /// let options = Some(ListImagesOptions{
    ///   all: true,
    ///   ..Default::default()
    /// });
    ///
    /// docker.list_images(options);
    /// ```
    pub fn list_images(
        &self,
        options: Option<ListImagesOptions>,
    ) -> impl Future<Item = Vec<APIImages>, Error = Error> {
        let url = "/images/json";

        self.process_into_value(url, Method::GET, options, None::<NoParams>)
    }
    */

    /// ---
    /// # Create Image
    ///
    /// Create an image by either pulling it from a registry or importing it.
    ///
    /// # Returns
    ///
    ///  - [Create Image Results](image/enum.CreateImageResults.html), wrapped in an asynchronous
    ///  Stream.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_unix_defaults().unwrap();
    /// use boondock::image::CreateImageOptions;
    ///
    /// use std::default::Default;
    ///
    /// let options = Some(CreateImageOptions{
    ///   from_image: "hello-world",
    ///   ..Default::default()
    /// });
    ///
    /// docker.create_image(options);
    ///
    /// // do some other work while the image is pulled from the docker hub...
    /// ```
    ///
    /// # Unsupported
    ///
    ///  - Import from tarball
    ///
    pub fn create_image<K, V>(
        &self,
        options: Option<impl CreateImageQueryParams<K, V>>,
    ) -> impl Stream<Item = CreateImageResults, Error = Error>
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let url = "/images/create";

        self.process_into_stream2(
            url,
            Method::POST,
            options.and_then(|o| match o.into_array() {
                Ok(res) => Some(res),
                Err(e) => None,
            }),
            None::<NoParams>,
        )
    }

    /*
    /// ---
    ///
    /// # Inspect Image
    ///
    /// Return low-level information about an image.
    ///
    /// # Returns
    ///
    ///  - [Image](image/struct.Image.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_unix_defaults().unwrap();
    ///
    /// use std::default::Default;
    ///
    /// docker.inspect_image("hello-world");
    /// ```
    pub fn inspect_image(&self, image_name: &str) -> impl Future<Item = Image, Error = Error> {
        let url = format!("/images/{}/json", image_name);

        self.process_into_value(&url, Method::GET, None::<NoParams>, None::<NoParams>)
    }

    /// ---
    ///
    /// # Prune Images
    ///
    /// Delete unused images.
    ///
    /// # Returns
    ///
    ///  - a [Prune Images Results](image/struct.PruneImagesResults.html), wrapped in a Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_unix_defaults().unwrap();
    ///
    /// docker.prune_images(None);
    /// ```
    pub fn prune_images(
        &self,
        options: Option<PruneImagesOptions>,
    ) -> impl Future<Item = PruneImagesResults, Error = Error> {
        let url = "/images/prune";

        self.process_into_value(&url, Method::POST, options, None::<NoParams>)
    }

    /// ---
    ///
    /// # Image History
    ///
    /// Return parent layers of an image.
    ///
    /// # Returns
    ///
    ///  - Vector of [Image History Results](image/struct.ImageHistoryResults.html), wrapped in a
    ///  Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    /// # let docker = Docker::connect_with_unix_defaults().unwrap();
    ///
    /// docker.image_history("hello-world");
    /// ```
    pub fn image_history(
        &self,
        image_name: &str,
    ) -> impl Future<Item = Vec<ImageHistory>, Error = Error> {
        let url = format!("/images/{}/history", image_name);

        self.process_into_value(&url, Method::GET, None::<NoParams>, None::<NoParams>)
    }

    /// ---
    ///
    /// # Search Images
    ///
    /// Search for an image on Docker Hub.
    ///
    /// # Returns
    ///
    ///  - Vector of [API Image Search](image/struct.APIImageSearch.html) results, wrapped in a
    ///  Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    ///
    /// use boondock::image::SearchImagesOptions;
    ///
    /// # let docker = Docker::connect_with_unix_defaults().unwrap();
    /// let search_options = SearchImagesOptions {
    ///     term: "hello-world".to_string(),
    ///     ..Default::default()
    /// };
    ///
    /// docker.search_images(search_options);
    /// ```
    pub fn search_images(
        &self,
        options: SearchImagesOptions,
    ) -> impl Future<Item = Vec<APIImageSearch>, Error = Error> {
        let url = "/images/search";

        self.process_into_value(url, Method::GET, Some(options), None::<NoParams>)
    }

    /// ---
    ///
    /// # Remove Image
    ///
    /// Remove an image, along with any untagged parent images that were referenced by that image.
    ///
    /// # Returns
    ///
    ///  - Vector of [Remove Image Results](image/struct.RemoveImageResults.html), wrapped in a
    ///  Future.
    ///
    /// # Examples
    ///
    /// ```rust,norun
    /// # use boondock::Docker;
    ///
    /// use boondock::image::RemoveImageOptions;
    ///
    /// # let docker = Docker::connect_with_unix_defaults().unwrap();
    /// let remove_options = RemoveImageOptions {
    ///     force: true,
    ///     ..Default::default()
    /// };
    ///
    /// docker.remove_image("hello-world", remove_options);
    /// ```
    pub fn remove_image(
        &self,
        image_name: &str,
        options: Option<RemoveImageOptions>,
    ) -> impl Future<Item = Vec<RemoveImageResults>, Error = Error> {
        let url = format!("/images/{}", image_name);

        self.process_into_value(&url, Method::DELETE, options, None::<NoParams>)
    }

    pub fn tag_image(
        &self,
        image_name: &str,
        options: Option<TagImageOptions>,
    ) -> impl Future<Item = (), Error = Error> {
        let url = format!("/images/{}/tag", image_name);

        self.process_into_void(&url, Method::POST, options, None::<NoParams>)
    }

    pub fn push_image(
        &self,
        image_name: &str,
        options: Option<PushImageOptions>,
    ) -> impl Future<Item = (), Error = Error> {
        let url = format!("/images/{}/push", image_name);

        self.process_into_void(&url, Method::POST, options, None::<NoParams>)
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use hyper_mock::SequentialConnector;
    use tokio;

    #[test]
    fn list_images() {
        let mut connector = SequentialConnector::default();
        connector.content.push(
       "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 310\r\n\r\n[{\"Containers\":-1,\"Created\":1484347856,\"Id\":\"sha256:48b5124b2768d2b917edcb640435044a97967015485e812545546cbed5cf0233\",\"Labels\":{},\"ParentId\":\"\",\"RepoDigests\":[\"hello-world@sha256:c5515758d4c5e1e838e9cd307f6c6a0d620b5e07e6f927b07d05f6d12a1ac8d7\"],\"RepoTags\":null,\"SharedSize\":-1,\"Size\":1840,\"VirtualSize\":1840}]\r\n\r\n".to_string()
     );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let options = Some(ListImagesOptions {
            all: true,
            ..Default::default()
        });

        let images = docker.list_images(options);

        let future = images
            .map_err(|e| panic!("error = {:?}", e))
            .map(|images| assert_eq!(images[0].size, 1840));

        tokio::runtime::run(future);
    }

    #[test]
    fn test_create_image() {
        let mut connector = SequentialConnector::default();
        connector.content.push(
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 542\r\n\r\n{\"status\":\"Pulling from library/hello-world\",\"id\":\"latest\"}\r\n{\"status\":\"Digest: sha256:0add3ace90ecb4adbf7777e9aacf18357296e799f81cabc9fde470971e499788\"}\r\n{\"status\":\"Pulling from library/hello-world\",\"id\":\"linux\"}\r\n{\"status\":\"Digest: sha256:d5c7d767f5ba807f9b363aa4db87d75ab030404a670880e16aedff16f605484b\"}\r\n{\"status\":\"Pulling from library/hello-world\",\"id\":\"nanoserver-1709\"}\r\n{\"errorDetail\":{\"message\":\"no matching manifest for unknown in the manifest list entries\"},\"error\":\"no matching manifest for unknown in the manifest list entries\"}\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let options = Some(CreateImageOptions {
            from_image: String::from("hello-world"),
            ..Default::default()
        });

        let stream = docker.create_image(options);

        let future = stream
            .into_future()
            .map_err(|e| panic!("error = {:?}", e.0))
            .map(|images| assert_eq!(images.0.is_some(), true));

        tokio::runtime::run(future);
    }

    #[test]
    fn test_inspect_image() {
        let mut connector = SequentialConnector::default();
        connector.content.push(
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 1744\r\n\r\n{\"Id\":\"sha256:4ab4c602aa5eed5528a6620ff18a1dc4faef0e1ab3a5eddeddb410714478c67f\",\"RepoTags\":[\"hello-world:latest\",\"hello-world:linux\"],\"RepoDigests\":[\"hello-world@sha256:0add3ace90ecb4adbf7777e9aacf18357296e799f81cabc9fde470971e499788\",\"hello-world@sha256:d5c7d767f5ba807f9b363aa4db87d75ab030404a670880e16aedff16f605484b\"],\"Parent\":\"\",\"Comment\":\"\",\"Created\":\"2018-09-07T19:25:39.809797627Z\",\"Container\":\"15c5544a385127276a51553acb81ed24a9429f9f61d6844db1fa34f46348e420\",\"ContainerConfig\":{\"Hostname\":\"15c5544a3851\",\"Domainname\":\"\",\"User\":\"\",\"AttachStdin\":false,\"AttachStdout\":false,\"AttachStderr\":false,\"Tty\":false,\"OpenStdin\":false,\"StdinOnce\":false,\"Env\":[\"PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin\"],\"Cmd\":[\"/bin/sh\",\"-c\",\"#(nop) \",\"CMD [\\\"/hello\\\"]\"],\"ArgsEscaped\":true,\"Image\":\"sha256:9a5813f1116c2426ead0a44bbec252bfc5c3d445402cc1442ce9194fc1397027\",\"Volumes\":null,\"WorkingDir\":\"\",\"Entrypoint\":null,\"OnBuild\":null,\"Labels\":{}},\"DockerVersion\":\"17.06.2-ce\",\"Author\":\"\",\"Config\":{\"Hostname\":\"\",\"Domainname\":\"\",\"User\":\"\",\"AttachStdin\":false,\"AttachStdout\":false,\"AttachStderr\":false,\"Tty\":false,\"OpenStdin\":false,\"StdinOnce\":false,\"Env\":[\"PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin\"],\"Cmd\":[\"/hello\"],\"ArgsEscaped\":true,\"Image\":\"sha256:9a5813f1116c2426ead0a44bbec252bfc5c3d445402cc1442ce9194fc1397027\",\"Volumes\":null,\"WorkingDir\":\"\",\"Entrypoint\":null,\"OnBuild\":null,\"Labels\":null},\"Architecture\":\"amd64\",\"Os\":\"linux\",\"Size\":1840,\"VirtualSize\":1840,\"GraphDriver\":{\"Data\":{\"MergedDir\":\"\",\"UpperDir\":\"\",\"WorkDir\":\"\"},\"Name\":\"overlay2\"},\"RootFS\":{\"Type\":\"layers\",\"Layers\":[\"sha256:428c97da766c4c13b19088a471de6b622b038f3ae8efa10ec5a37d6d31a2df0b\"]},\"Metadata\":{\"LastTagTime\":\"0001-01-01T00:00:00Z\"}}\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let image = docker.inspect_image("hello-world");

        let future = image
            .map_err(|e| panic!("error = {:?}", e))
            .map(|image| assert_eq!(image.architecture, "amd64"));

        tokio::runtime::run(future);
    }

    #[test]
    fn test_prune_images() {
        let mut connector = SequentialConnector::default();
        connector.content.push(
            "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 42\r\n\r\n{\"ImagesDeleted\":null,\"SpaceReclaimed\":40}\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let prune_images_results = docker.prune_images(None);

        let future = prune_images_results
            .map_err(|e| panic!("error = {:?}", e))
            .map(|image| assert_eq!(image.space_reclaimed, 40));

        tokio::runtime::run(future);
    }

    #[test]
    fn test_image_history() {
        let mut connector = SequentialConnector::default();
        connector.content.push(
        "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 415\r\n\r\n[{\"Comment\":\"\",\"Created\":1536348339,\"CreatedBy\":\"/bin/sh -c #(nop)  CMD [\\\"/hello\\\"]\",\"Id\":\"sha256:4ab4c602aa5eed5528a6620ff18a1dc4faef0e1ab3a5eddeddb410714478c67f\",\"Size\":0,\"Tags\":[\"hello-world:latest\",\"hello-world:linux\"]},{\"Comment\":\"\",\"Created\":1536348339,\"CreatedBy\":\"/bin/sh -c #(nop) COPY file:9824c33ef192ac944822908370af9f04ab049bfa5c10724e4f727206f5167094 in / \",\"Id\":\"<missing>\",\"Size\":1840,\"Tags\":null}]\r\n\r\n".to_string()
      );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let image_history_results = docker.image_history("hello-world");

        let future = image_history_results
            .map_err(|e| panic!("error = {:?}", e))
            .map(|vec| {
                assert!(vec.into_iter().take(1).any(|history| {
                    history.tags.unwrap_or(vec![String::new()])[0] == "hello-world:latest"
                }))
            });

        tokio::runtime::run(future);
    }

    #[test]
    fn test_search_images() {
        let mut connector = SequentialConnector::default();
        connector.content.push(
          "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 148\r\n\r\n[{\"star_count\":660,\"is_official\":true,\"name\":\"hello-world\",\"is_automated\":false,\"description\":\"Hello World! (an example of minimal Dockerization)\"}]\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let search_options = SearchImagesOptions {
            term: "hello-world".to_string(),
            ..Default::default()
        };

        let search_results = docker.search_images(search_options);

        let future = search_results
            .map_err(|e| panic!("error = {:?}", e))
            .map(|vec| {
                assert!(
                    vec.into_iter()
                        .any(|api_image| &api_image.name == "hello-world")
                )
            });

        tokio::runtime::run(future);
    }

    #[test]
    fn test_remove_image() {
        let mut connector = SequentialConnector::default();
        connector.content.push(
          "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 35\r\n\r\n[{\"Untagged\":\"hello-world:latest\"}]\r\n\r\n".to_string()
        );

        let docker = Docker::connect_with(connector, String::new()).unwrap();

        let remove_options = RemoveImageOptions {
            noprune: true,
            ..Default::default()
        };

        let remove_results = docker.remove_image("hello-world", Some(remove_options));

        let future = remove_results
            .map_err(|e| panic!("error = {:?}", e))
            .map(|vec| {
                assert!(vec.into_iter().any(|result| match result {
                    RemoveImageResults::RemoveImageUntagged { untagged } => {
                        untagged == "hello-world:latest"
                    }
                    _ => false,
                }))
            });

        tokio::runtime::run(future);
    }
    */
}
