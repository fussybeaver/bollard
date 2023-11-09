//! GRPC plumbing to interact with Docker's buildkit client
#![cfg(feature = "buildkit")]
#![allow(dead_code)]

/// A package of GRPC buildkit connection implementations
pub mod driver;
/// Errors for the GRPC modules
pub mod error;
/// End-user buildkit export functions
pub mod export;
/// Internal interfaces to convert types for GRPC communication
pub(crate) mod io;

use crate::auth::DockerCredentials;
use crate::health::health_check_response::ServingStatus;
use crate::health::health_server::Health;
use crate::health::{HealthCheckRequest, HealthCheckResponse};
use crate::moby::filesync::v1::auth_server::Auth;
use crate::moby::filesync::v1::file_send_server::FileSend;
use crate::moby::filesync::v1::{
    BytesMessage as FileSyncBytesMessage, CredentialsRequest, CredentialsResponse,
    FetchTokenRequest, FetchTokenResponse, GetTokenAuthorityRequest, GetTokenAuthorityResponse,
    VerifyTokenAuthorityRequest, VerifyTokenAuthorityResponse,
};
use crate::moby::upload::v1::upload_server::{Upload, UploadServer};
use crate::moby::upload::v1::BytesMessage as UploadBytesMessage;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll};

use bollard_buildkit_proto::moby::filesync::v1::auth_server::AuthServer;
use bollard_buildkit_proto::moby::filesync::v1::file_send_server::FileSendServer;
use futures_core::Stream;
use hyper::client::HttpConnector;
use rand::RngCore;
use tonic::transport::NamedService;
use tonic::{Code, Request, Response, Status, Streaming};

use futures_util::{StreamExt, TryFutureExt};
use tokio::io::AsyncWriteExt;

use http::request::Builder;
use hyper::{Body, Client, Method};
use std::future::Future;
use tower::Service;

use self::error::GrpcAuthError;
use self::io::GrpcTransport;

/// A static dispatch wrapper for GRPC implementations to generated GRPC traits
#[derive(Debug)]
pub(crate) enum GrpcServer {
    Auth(AuthServer<AuthProvider>),
    Upload(UploadServer<UploadProvider>),
    FileSend(FileSendServer<FileSendImpl>),
}

impl GrpcServer {
    pub fn append(
        self,
        builder: tonic::transport::server::Router,
    ) -> tonic::transport::server::Router {
        match self {
            GrpcServer::Auth(auth_server) => builder.add_service(auth_server),
            GrpcServer::Upload(upload_server) => builder.add_service(upload_server),
            GrpcServer::FileSend(file_send_server) => builder.add_service(file_send_server),
        }
    }

    pub fn names(&self) -> Vec<String> {
        match self {
            GrpcServer::Auth(_auth_server) => {
                vec![
                    format!("/{}/credentials", AuthServer::<AuthProvider>::NAME),
                    format!("/{}/fetch_token", AuthServer::<AuthProvider>::NAME),
                ]
            }
            GrpcServer::Upload(_upload_server) => {
                vec![format!("/{}/pull", UploadServer::<UploadProvider>::NAME)]
            }
            GrpcServer::FileSend(_file_send_server) => {
                vec![format!(
                    "/{}/diffcopy",
                    FileSendServer::<FileSendImpl>::NAME
                )]
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct HealthServerImpl {
    service_map: HashMap<String, ServingStatus>,
    shutdown: bool,
}

impl HealthServerImpl {
    pub fn new() -> Self {
        let mut service_map = HashMap::new();
        service_map.insert(String::from(""), ServingStatus::Serving);
        Self {
            service_map,
            shutdown: false,
        }
    }

    pub fn shutdown(mut self) {
        self.shutdown = true;
        for (_, val) in self.service_map.iter_mut() {
            *val = ServingStatus::NotServing;
        }
    }
}

#[tonic::async_trait]
impl Health for HealthServerImpl {
    type WatchStream = Pin<Box<dyn Stream<Item = Result<HealthCheckResponse, Status>> + Send>>;
    async fn check(
        &self,
        request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        trace!("Received GRPC Health Request: {:#?}", request);
        if let Some(status) = self.service_map.get(&request.get_ref().service) {
            Ok(Response::new(HealthCheckResponse {
                status: *status as i32,
            }))
        } else {
            Err(Status::new(Code::NotFound, "unknown service"))
        }
    }
    async fn watch(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<Self::WatchStream>, Status> {
        unimplemented!();
    }
}

#[derive(Clone, Debug)]
pub(crate) struct FileSendImpl {
    pub(crate) dest: PathBuf,
}

impl FileSendImpl {
    pub fn new(dest: &Path) -> Self {
        Self {
            dest: dest.to_owned(),
        }
    }
}

#[tonic::async_trait]
impl FileSend for FileSendImpl {
    type DiffCopyStream = Pin<Box<dyn Stream<Item = Result<FileSyncBytesMessage, Status>> + Send>>;
    async fn diff_copy(
        &self,
        request: Request<Streaming<FileSyncBytesMessage>>,
    ) -> Result<Response<Self::DiffCopyStream>, Status> {
        trace!("Protobuf FileSend diff_copy triggered: {:#?}", request);

        let path = self.dest.as_path();

        let mut in_stream = request.into_inner();

        let mut file = tokio::fs::File::create(path).await?;

        while let Some(result) = in_stream.next().await {
            match result {
                Ok(v) => {
                    file.write_all(&v.data).await?;
                }
                Err(err) => return Err(err.into()),
            }
        }

        Ok(Response::new(Box::pin(futures_util::stream::empty())))
    }
}

#[derive(Debug)]
pub(crate) struct UploadProvider {
    pub(crate) store: HashMap<String, Vec<u8>>,
}

impl UploadProvider {
    pub fn new() -> Self {
        Self {
            store: HashMap::new(),
        }
    }

    pub fn add(&mut self, reader: Vec<u8>) -> String {
        let id = new_id();
        let key = format!("http://buildkit-session/{}", id);

        self.store.insert(format!("/{}", id), reader);
        key
    }
}

#[tonic::async_trait]
impl Upload for UploadProvider {
    type PullStream = Pin<Box<dyn Stream<Item = Result<UploadBytesMessage, Status>> + Send>>;

    async fn pull(
        &self,
        request: Request<Streaming<UploadBytesMessage>>,
    ) -> Result<Response<Self::PullStream>, Status> {
        let key = request
            .metadata()
            .get("urlpath")
            .and_then(|key| key.to_str().ok())
            .map(String::from)
            .and_then(|str| self.store.get(&str));
        if let Some(read) = key {
            let out_stream =
                futures_util::stream::once(futures_util::future::ok(UploadBytesMessage {
                    data: read.to_owned(),
                }));

            Ok(Response::new(Box::pin(out_stream)))
        } else {
            Err(Status::invalid_argument(
                "invalid 'urlpath' in uploadprovider request",
            ))
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct AuthProvider {
    auth_config_cache: HashMap<String, DockerCredentials>,
    registry_token: Option<String>,
}

const DEFAULT_TOKEN_EXPIRATION: i64 = 60;
const DOCKER_HUB_REGISTRY_HOST: &str = "https://index.docker.io/v1/";
const DOCKER_HUB_CONFIG_FILE_KEY: &str = "registry-1.docker.io";

enum TokenExpiry {
    DEFAULT,
    EXPIRES(i64),
}

struct TokenOptions {
    realm: String,
    service: String,
    scopes: Vec<String>,
    username: String,
    secret: String,
    fetch_refresh_token: bool,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
struct OAuthTokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
    #[cfg_attr(feature = "schemars", schemars(with = "crate::models::Rfc3339"))]
    issued_at: chrono::DateTime<chrono::Utc>,
    scope: String,
}

impl AuthProvider {
    pub fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    pub(crate) fn set_docker_credentials(
        &mut self,
        host: &str,
        docker_credentials: DockerCredentials,
    ) {
        self.auth_config_cache
            .insert(String::from(host), docker_credentials);
    }

    fn get_auth_config(&self, mut host: &str) -> Result<DockerCredentials, Status> {
        if host == DOCKER_HUB_REGISTRY_HOST {
            host = DOCKER_HUB_CONFIG_FILE_KEY;
        }

        if let Some(creds) = self.auth_config_cache.get(host) {
            Ok(DockerCredentials::to_owned(creds))
        } else {
            Err(Status::permission_denied(format!(
                "Could not find credentials for {host}"
            )))
        }
    }

    fn to_token_response(
        &self,
        token: &str,
        issued_at: chrono::DateTime<chrono::Utc>,
        expires: TokenExpiry,
    ) -> FetchTokenResponse {
        let expires = match expires {
            TokenExpiry::DEFAULT => DEFAULT_TOKEN_EXPIRATION,
            TokenExpiry::EXPIRES(expiry) => expiry,
        };

        FetchTokenResponse {
            token: String::from(token),
            expires_in: expires,
            issued_at: issued_at.timestamp(),
        }
    }

    fn get_credentials(&self, host: &str) -> Result<CredentialsResponse, Status> {
        let ac = self.get_auth_config(host)?;

        match ac {
            DockerCredentials {
                identitytoken: Some(identitytoken),
                ..
            } => Ok(CredentialsResponse {
                username: String::new(),
                secret: identitytoken,
            }),
            DockerCredentials {
                username: Some(username),
                password: Some(password),
                ..
            } => Ok(CredentialsResponse {
                username,
                secret: password,
            }),
            DockerCredentials { .. } => Err(Status::unknown("Invalid DockerCredentials provided")),
        }
    }

    fn ssl_client() -> Result<Client<hyper_rustls::HttpsConnector<HttpConnector>>, GrpcAuthError> {
        let mut root_store = rustls::RootCertStore::empty();
        for cert in rustls_native_certs::load_native_certs()? {
            root_store.add(&rustls::Certificate(cert.0))?;
        }

        let config = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let mut http_connector = HttpConnector::new();
        http_connector.enforce_http(false);

        let https_connector: hyper_rustls::HttpsConnector<HttpConnector> =
            hyper_rustls::HttpsConnector::from((http_connector, config));

        let client_builder = Client::builder();
        let client = client_builder.build(https_connector);

        Ok(client)
    }

    async fn fetch_token_with_oauth(
        &self,
        opts: &TokenOptions,
    ) -> Result<OAuthTokenResponse, GrpcAuthError> {
        let mut form = vec![];
        form.push(("client_id", "bollard-client"));
        let scopes = opts.scopes.join(" ");
        if !opts.scopes.is_empty() {
            form.push(("scope", &scopes));
        }
        form.push(("service", &opts.service));
        if opts.username.is_empty() {
            form.push(("grant_type", "refresh_token"));
            form.push(("refresh_token", &opts.secret));
        } else {
            form.push(("grant_type", "password"));
            form.push(("username", &opts.username));
            form.push(("password", &opts.secret));
        }
        if opts.fetch_refresh_token {
            form.push(("access_type", "offline"));
        }

        let params = serde_urlencoded::to_string(form)?;

        let client = Self::ssl_client()?;

        let full_uri = format!("{}?{}", opts.realm, &params);
        let request_uri: hyper::Uri = full_uri.try_into()?;
        let request = hyper::Request::post(request_uri).body(Body::empty())?;

        let response = client.request(request).await?;

        let status = response.status().as_u16();
        if status < 200 || status >= 400 {
            // return custom error
            return Err(GrpcAuthError::BadRegistryResponse {
                status_code: status,
            });
        }

        let bytes = hyper::body::to_bytes(response.into_body()).await?;

        let oauth_token = serde_json::from_slice::<OAuthTokenResponse>(&bytes)?;

        Ok(oauth_token)
    }
}

#[tonic::async_trait]
impl Auth for AuthProvider {
    async fn credentials(
        &self,
        request: Request<CredentialsRequest>,
    ) -> Result<Response<CredentialsResponse>, Status> {
        let host = request.get_ref().host.as_ref();

        Ok(Response::new(self.get_credentials(host)?))
    }

    async fn fetch_token(
        &self,
        request: Request<FetchTokenRequest>,
    ) -> Result<Response<FetchTokenResponse>, Status> {
        let FetchTokenRequest {
            client_id: _,
            host,
            realm,
            service,
            scopes,
        } = request.get_ref();

        let creds = self.get_credentials(host)?;

        // check for statically configured bearer token
        if let Some(token) = self.registry_token.as_ref() {
            Ok(Response::new(self.to_token_response(
                token,
                chrono::Utc::now(),
                TokenExpiry::DEFAULT,
            )))
        } else {
            let to = TokenOptions {
                realm: String::clone(realm),
                service: String::clone(service),
                scopes: Vec::clone(scopes),
                username: creds.username,
                secret: creds.secret,
                fetch_refresh_token: false,
            };

            match self.fetch_token_with_oauth(&to).await {
                Ok(res) => Ok(Response::new(self.to_token_response(
                    &res.access_token,
                    res.issued_at,
                    TokenExpiry::EXPIRES(res.expires_in),
                ))),
                Err(e) => Err(Status::from_error(Box::new(e))),
            }
        }
    }

    async fn get_token_authority(
        &self,
        _request: Request<GetTokenAuthorityRequest>,
    ) -> Result<Response<GetTokenAuthorityResponse>, Status> {
        unimplemented!()
    }
    async fn verify_token_authority(
        &self,
        _request: Request<VerifyTokenAuthorityRequest>,
    ) -> Result<Response<VerifyTokenAuthorityResponse>, Status> {
        unimplemented!()
    }
}

pub(crate) struct GrpcClient {
    pub(crate) client: crate::Docker,
    pub(crate) session_id: String,
}

impl Service<http::Uri> for GrpcClient {
    type Response = GrpcTransport;
    type Error = error::GrpcError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: http::Uri) -> Self::Future {
        // create the body
        let opt: Option<serde_json::Value> = None;
        let url = "/grpc";
        let client = self.client.clone();
        let req = client.build_request(
            &url,
            Builder::new()
                .method(Method::POST)
                .header("Connection", "Upgrade")
                .header("Upgrade", "h2c")
                .header("X-Docker-Expose-Session-Uuid", &self.session_id),
            opt,
            Ok(Body::empty()),
        );
        let fut = async move {
            client
                .process_upgraded(req)
                .await
                .and_then(|(read, write)| {
                    let output = Box::pin(read);
                    let input = Box::pin(write);
                    Ok(GrpcTransport {
                        read: output,
                        write: input,
                    })
                })
        };

        // Return the response as an immediate future
        Box::pin(fut.map_err(From::from))
    }
}

// Reference: https://github.com/moby/buildkit/blob/master/identity/randomid.go
pub(crate) fn new_id() -> String {
    let mut p: [u8; 17] = Default::default();
    rand::thread_rng().fill_bytes(&mut p);
    p[0] |= 0x80; // set high bit to avoid the need for padding
    num::BigInt::from_bytes_be(num::bigint::Sign::Plus, &p[..]).to_str_radix(36)[1..26].to_string()
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_new_id() {
        let s = super::new_id();
        assert_eq!(s.len(), 25);
    }
}
