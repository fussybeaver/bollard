//! [![crates.io](https://img.shields.io/crates/v/bollard.svg)](https://crates.io/crates/bollard)
//! [![license](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
//! [![circle-ci](https://circleci.com/gh/fussybeaver/bollard/tree/master.svg?style=svg)](https://circleci.com/gh/fussybeaver/bollard/tree/master)
//! [![appveyor](https://ci.appveyor.com/api/projects/status/n5khebyfae0u1sbv/branch/master?svg=true)](https://ci.appveyor.com/project/fussybeaver/boondock)
//! [![docs](https://docs.rs/bollard/badge.svg)](https://docs.rs/bollard/)
//!
//! # Bollard: an asynchronous rust client library for the docker API
//!
//! Bollard leverages the latest [Hyper](https://github.com/hyperium/hyper) and
//! [Tokio](https://github.com/tokio-rs/tokio) improvements for an asynchronous API containing
//! futures, streams and the async/await paradigm.
//!
//! This library features Windows support through [Named
//! Pipes](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipes) and HTTPS support through optional
//! [Rustls](https://github.com/rustls/rustls) bindings. Serialization types for interfacing with
//! [Docker](https://github.com/moby/moby) and [Buildkit](https://github.com/moby/buildkit) are
//! generated through OpenAPI, protobuf and upstream documentation.
//!
//! # Install
//!
//! Add the following to your `Cargo.toml` file
//!
//! ```nocompile
//! [dependencies]
//! bollard = "*"
//! ```
//!
//! # API
//! ## Documentation
//!
//! [API docs](https://docs.rs/bollard/).
//!
//! ## Feature flags
//!
//!  - `ssl`: enable SSL support through [Rustls](https://github.com/rustls/rustls) with the [ring](https://github.com/briansmith/ring) provider.
//!  - `aws-lc-rs`: enable SSL support through [Rustls](https://github.com/rustls/rustls) with the [aws-lc-rs](https://github.com/aws/aws-lc-rs) provider.
//!  - `ssl_providerless`: enable SSL support through [Rustls](https://github.com/rustls/rustls) without installing a [CryptoProvider](https://docs.rs/rustls/0.23.12/rustls/crypto/struct.CryptoProvider.html). You are responsible to do so.
//!  - `chrono`: enable [Chrono](https://github.com/chronotope/chrono) for `DateTime` types.
//!  - `time`: enable [Time 0.3](https://github.com/time-rs/time) for `DateTime` types.
//!  - `buildkit`: use [Buildkit](https://github.com/moby/buildkit) instead of
//!    [Docker](https://github.com/moby/moby) when building images.
//!  - `json_data_content`: Add JSON to errors on serialization failures.
//!  - `webpki`: Use mozilla's root certificates instead of native root certs provided by the OS.
//!
//! ## Version
//!
//! The [Docker API](https://docs.docker.com/engine/api/v1.48/) used by Bollard is using the latest
//! `1.48` documentation schema published by the [moby](https://github.com/moby/moby) project to
//! generate its serialization interface.
//!
//! This library also supports [version
//! negotiation](https://docs.rs/bollard/latest/bollard/struct.Docker.html#method.negotiate_version),
//! to allow downgrading to an older API version.
//!
//! # Usage
//!
//! ## Connecting with the docker daemon
//!
//! Connect to the docker server according to your architecture and security remit.
//!
//! ### Socket
//!
//! The client will connect to the standard unix socket location `/var/run/docker.sock` or Windows
//! named pipe location `//./pipe/docker_engine`.
//!
//! ```rust
//! use bollard::Docker;
//! #[cfg(unix)]
//! Docker::connect_with_socket_defaults();
//! ```
//!
//! Use the `Docker::connect_with_socket` method API to parameterise this interface.
//!
//! ### Local
//!
//! The client will connect to the OS specific handler it is compiled for.
//!
//! This is a convenience for localhost environment that should run on multiple
//! operating systems.
//!
//! ```rust
//! use bollard::Docker;
//! Docker::connect_with_local_defaults();
//! ```
//!
//! Use the `Docker::connect_with_local` method API to parameterise this interface.
//!
//! ### HTTP
//!
//! The client will connect to the location pointed to by `DOCKER_HOST` environment variable, or
//! `localhost:2375` if missing.
//!
//! ```rust
//! use bollard::Docker;
//! Docker::connect_with_http_defaults();
//! ```
//!
//! Use the `Docker::connect_with_http` method API to parameterise the interface.
//!
//! ### SSL via Rustls
//!
//! The client will connect to the location pointed to by `DOCKER_HOST` environment variable, or
//! `localhost:2375` if missing.
//!
//! The location pointed to by the `DOCKER_CERT_PATH` environment variable is searched for
//! certificates - `key.pem` for the private key, `cert.pem` for the server certificate and
//! `ca.pem` for the certificate authority chain.
//!
//! ```rust
//! use bollard::Docker;
//! #[cfg(feature = "ssl")]
//! Docker::connect_with_ssl_defaults();
//! ```
//!
//! Use the `Docker::connect_with_ssl` method API to parameterise the interface.
//!
//! ## Examples
//!
//! Note: all these examples need a [Tokio
//! Runtime](https://tokio.rs/).
//!
//! ### Version
//!
//! First, check that the API is working with your server:
//!
//! ```rust,no_run
//! use bollard::Docker;
//!
//! use futures_util::future::FutureExt;
//!
//! // Use a connection function described above
//! // let docker = Docker::connect_...;
//! # let docker = Docker::connect_with_local_defaults().unwrap();
//!
//! async move {
//!     let version = docker.version().await.unwrap();
//!     println!("{:?}", version);
//! };
//! ```
//!
//! ### Listing images
//!
//! To list docker images available on the Docker server:
//!
//! ```rust,no_run
//! use bollard::Docker;
//! use bollard::image::ListImagesOptions;
//!
//! use futures_util::future::FutureExt;
//!
//! use std::default::Default;
//!
//! // Use a connection function described above
//! // let docker = Docker::connect_...;
//! # let docker = Docker::connect_with_local_defaults().unwrap();
//!
//! async move {
//!     let images = &docker.list_images(Some(ListImagesOptions::<String> {
//!         all: true,
//!         ..Default::default()
//!     })).await.unwrap();
//!
//!     for image in images {
//!         println!("-> {:?}", image);
//!     }
//! };
//! ```
//!
//! ## Streaming Stats
//!
//! To receive a stream of stats for a running container.
//!
//! ```rust,no_run
//! use bollard::Docker;
//! use bollard::query_parameters::StatsOptionsBuilder;
//!
//! use futures_util::stream::TryStreamExt;
//!
//! use std::default::Default;
//!
//! // Use a connection function described above
//! // let docker = Docker::connect_...;
//! # let docker = Docker::connect_with_local_defaults().unwrap();
//!
//! async move {
//!     let stats = &docker.stats("postgres", Some(
//!       StatsOptionsBuilder::default().stream(true).build()
//!     )).try_collect::<Vec<_>>().await.unwrap();
//!
//!     for stat in stats {
//!         println!("{} - mem total: {:?} | mem usage: {:?}",
//!             stat.name.as_ref().unwrap(),
//!             stat.memory_stats.as_ref().unwrap().max_usage,
//!             stat.memory_stats.as_ref().unwrap().usage);
//!     }
//! };
//! ```
//!
//! # Examples
//!
//! Further examples are available in the [examples
//! folder](https://github.com/fussybeaver/bollard/tree/master/examples), or the [integration/unit
//! tests](https://github.com/fussybeaver/bollard/tree/master/tests).
//!
//! # Development
//!
//! Contributions are welcome, please observe the following.
//!
//! ## Building the proto models
//!
//! Serialization models for the buildkit feature are generated through the [Tonic
//! library](https://github.com/hyperium/tonic/). To generate these files, use the
//! following in the `codegen/proto` folder:
//!
//! ```bash
//! cargo run --bin gen --features build
//! ```
//!
//! ## Building the swagger models
//!
//! Serialization models are generated through the [Swagger
//! library](https://github.com/swagger-api/swagger-codegen/). To generate these files, use the
//! following in the `codegen/swagger` folder:
//!
//! ```bash
//! mvn -D org.slf4j.simpleLogger.defaultLogLevel=error compiler:compile generate-resources
//! ```
//!
//! # Integration tests
//!
//! Running the integration tests by default requires a running docker registry, with images tagged
//! and pushed there. To disable this behaviour, set the `DISABLE_REGISTRY` environment variable.
//!
//! ```bash
//! docker run -d --restart always --name registry -p 5000:5000 registry:2
//! docker pull hello-world:linux
//! docker pull fussybeaver/uhttpd
//! docker pull alpine
//! docker tag hello-world:linux localhost:5000/hello-world:linux
//! docker tag fussybeaver/uhttpd localhost:5000/fussybeaver/uhttpd
//! docker tag alpine localhost:5000/alpine
//! docker push localhost:5000/hello-world:linux
//! docker push localhost:5000/fussybeaver/uhttpd
//! docker push localhost:5000/alpine
//! docker swarm init
//! REGISTRY_HTTP_ADDR=localhost:5000 cargo test -- --test-threads 1
//! ```
#![deny(
    missing_docs,
    missing_debug_implementations,
    missing_copy_implementations,
    trivial_casts,
    trivial_numeric_casts,
    //unstable_features,
    unused_import_braces,
)]
#![allow(
    clippy::upper_case_acronyms,
    clippy::derive_partial_eq_without_eq,
    async_fn_in_trait
)]
#![warn(rust_2018_idioms)]

// declare modules
pub mod auth;
pub mod container;
mod docker;
pub mod errors;
pub mod exec;
pub mod image;
pub mod network;
pub mod node;
mod read;
pub mod secret;
pub mod service;
#[cfg(feature = "ssh")]
mod ssh;
pub mod swarm;
pub mod system;
pub mod task;
mod uri;
pub mod volume;

pub mod grpc;

// publicly re-export
pub use crate::docker::{
    body_full, body_stream, body_try_stream, BollardRequest, ClientVersion, Docker,
    API_DEFAULT_VERSION,
};
pub use bollard_stubs::models;
pub use bollard_stubs::query_parameters;

#[cfg(feature = "buildkit")]
pub use bollard_buildkit_proto::fsutil;

#[cfg(feature = "buildkit")]
pub use bollard_buildkit_proto::health;

#[cfg(feature = "buildkit")]
pub use bollard_buildkit_proto::moby;
