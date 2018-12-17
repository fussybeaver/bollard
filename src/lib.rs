#![deny(
    missing_docs, missing_debug_implementations, missing_copy_implementations, trivial_casts,
    trivial_numeric_casts, unstable_features, unused_import_braces, unused_qualifications
)]
//! [![crates.io](https://img.shields.io/crates/v/bollard.svg)](https://crates.io/crates/bollard)
//! [![license](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
//! [![circle-ci](https://circleci.com/gh/fussybeaver/bollard.svg?style=svg)](https://circleci.com/gh/fussybeaver/bollard)
//! [![appveyor](https://ci.appveyor.com/api/projects/status/n5khebyfae0u1sbv?svg=true)](https://ci.appveyor.com/project/fussybeaver/boondock)
//! [![docs](https://docs.rs/bollard/badge.svg?version=0.1.0)](https://docs.rs/bollard/)
//!
//! # Bollard: an asynchronous rust client library for the docker API
//!
//! Bollard leverages the latest [Hyper](https://github.com/hyperium/hyper) and
//! [Tokio](https://github.com/tokio-rs/tokio) improvements for an asynchronous API containing
//! futures and streams.
//!
//! The library also features Windows support through Named Pipes and HTTPS support through
//! optional SSL bindings or a native TLS implementation.
//!
//! # Install
//!
//! Add the following to your `Cargo.toml` file
//!
//! ```nocompile
//! [dependencies]
//! bollard = "0.1"
//! ```
//!
//! # API documentation
//!
//! [API docs](https://docs.rs/bollard/)
//!
//! # Usage
//!
//! ## Connecting with the docker daemon
//!
//! Connect to the docker server according to your architecture and security remit.
//!
//! ### Unix socket
//!
//! The client will connect to the standard unix socket location `/var/run/docker.sock`. Use the
//! `Docker::connect_with_unix` method API to parameterise the
//! interface.
//!
//! ```rust,norun
//! use bollard::Docker;
//! #[cfg(unix)]
//! Docker::connect_with_unix_defaults();
//! ```
//!
//! ### Windows named pipe
//!
//! The client will connect to the standard windows pipe location `\\.\pipe\docker_engine`. Use the
//! `Docker::connect_with_name_pipe` method API
//! to parameterise the interface.
//!
//! ```rust,norun
//! use bollard::Docker;
//! #[cfg(windows)]
//! Docker::connect_with_named_pipe_defaults();
//! ```
//!
//! ### HTTP
//!
//! The client will connect to the location pointed to by `DOCKER_HOST` environment variable, or
//! `localhost:2375` if missing. Use the
//! `Docker::connect_with_http` method API to
//! parameterise the interface.
//!
//! ```rust,norun
//! use bollard::Docker;
//! Docker::connect_with_http_defaults();
//! ```
//!
//! ### SSL via openssl
//!
//! Openssl is switched off by default, and can be enabled through the `ssl` cargo feature.
//!
//! The client will connect to the location pointed to by `DOCKER_HOST` environment variable, or
//! `localhost:2375` if missing.
//!
//! The location pointed to by the `DOCKER_CERT_PATH` environment variable is searched for
//! certificates - `key.pem` for the private key, `cert.pem` for the server certificate and
//! `ca.pem` for the certificate authority chain.
//!
//! Use the `Docker::connect_with_ssl` method API
//! to parameterise the interface.
//!
//! ```rust,norun
//! use bollard::Docker;
//! #[cfg(feature = "openssl")]
//! Docker::connect_with_ssl_defaults();
//! ```
//!
//! ### TLS
//!
//! Native TLS allows you to avoid the SSL bindings.
//!
//! The client will connect to the location pointed to by `DOCKER_HOST` environment variable, or
//! `localhost:2375` if missing.
//!
//! The location pointed to by the `DOCKER_CERT_PATH` environment variable is searched for
//! certificates - `identity.pfx` for the PKCS #12 archive and `ca.pem` for the certificate
//! authority chain.
//!
//! Use the `Docker::connect_with_ssl` method API
//! to parameterise the interface.
//!
//! ```rust,norun
//! use bollard::Docker;
//! Docker::connect_with_tls_defaults();
//! ```
//!
//! ## Examples
//!
//! Note: all these examples need a [Tokio
//! Runtime](https://tokio.rs/docs/getting-started/runtime/). A small example about how to use
//! Tokio is below.
//!
//! ### Version
//!
//! First, check that the API is working with your server:
//!
//! ```rust
//! # extern crate bollard;
//! # extern crate futures;
//! # extern crate yup_hyper_mock;
//! # fn main () {
//! use futures::Future;
//!
//! use bollard::Docker;
//!
//! // Use a connection function described above
//! // let docker = Docker::connect_...;
//! # use yup_hyper_mock::SequentialConnector;
//! # let mut connector = SequentialConnector::default();
//! # let docker = Docker::connect_with(connector, "localhost".to_string(), 5).unwrap();
//!
//! docker.version()
//!     .map(|version| {
//!         println!("{:?}", version);
//!     });
//! # }
//! ```
//!
//! ### Listing images
//!
//! To list docker images available on the Docker server:
//!
//! ```rust
//! # extern crate bollard;
//! # extern crate futures;
//! # extern crate yup_hyper_mock;
//! # fn main () {
//! use futures::Future;
//!
//! use bollard::Docker;
//! use bollard::image::ListImagesOptions;
//!
//! use std::default::Default;
//!
//! // Use a connection function described above
//! // let docker = Docker::connect_...;
//! # use yup_hyper_mock::SequentialConnector;
//! # let mut connector = SequentialConnector::default();
//! # let docker = Docker::connect_with(connector, "localhost".to_string(), 5).unwrap();
//!
//! docker.list_images(Some(ListImagesOptions::<String> {
//!    all: true,
//!    ..Default::default()
//! }))
//!   .map(|images| {
//!        for i in images {
//!            println!("-> {:?}", i);
//!        }
//!    });
//! # }
//! ```
//!
//! ## Streaming Stats
//!
//! To receive a stream of stats for a running container.
//!
//! ```rust
//! # extern crate bollard;
//! # extern crate futures;
//! # extern crate yup_hyper_mock;
//! # fn main () {
//! use futures::stream::Stream;
//!
//! use bollard::Docker;
//! use bollard::container::StatsOptions;
//!
//! use std::default::Default;
//!
//! // Use a connection function described above
//! // let docker = Docker::connect_...;
//! # use yup_hyper_mock::SequentialConnector;
//! # let mut connector = SequentialConnector::default();
//! # let docker = Docker::connect_with(connector, "localhost".to_string(), 5).unwrap();
//!
//! docker.stats("postgres", Some(StatsOptions {
//!    stream: true,
//!    ..Default::default()
//! }))
//!   .map(|stat| {
//!         println!("{} - mem total: {:?} | mem usage: {:?}",
//!             stat.name,
//!             stat.memory_stats.max_usage,
//!             stat.memory_stats.usage);
//!    });
//! # }
//! ```
//!
//! ## Chaining docker commands
//!
//! It's sometimes more convenient to chain a string of Docker API calls. The `DockerChain` API
//! will return an instance of itself in the return call.
//!
//! ```rust
//! # extern crate bollard;
//! # extern crate tokio;
//! # extern crate yup_hyper_mock;
//! # fn main () {
//! use bollard::Docker;
//! use bollard::image::CreateImageOptions;
//! use bollard::container::CreateContainerOptions;
//! use bollard::container::Config;
//!
//! use tokio::prelude::Future;
//!
//! use std::default::Default;
//!
//! // Use a connection function described above
//! // let docker = Docker::connect_...;
//! # use yup_hyper_mock::SequentialConnector;
//! # let mut connector = SequentialConnector::default();
//! # let docker = Docker::connect_with(connector, "localhost".to_string(), 5).unwrap();
//! docker.chain().create_image(Some(CreateImageOptions{
//!     from_image: "hello-world",
//!     ..Default::default()
//! })).and_then(|(docker, _)|
//!     docker.create_container(
//!         None::<CreateContainerOptions<String>>,
//!         Config {
//!             image: Some("hello-world"),
//!             cmd: vec!["/hello"],
//!             ..Default::default()
//!         }));
//! # }
//! ```
//!
//! # Examples
//!
//! Further examples are available in the examples folder, or the integration/unit tests.
//!
//! ## A Primer on the Tokio Runtime
//!
//! In order to use the API effectively, you will need to be familiar with the [Tokio
//! Runtime](https://tokio.rs/docs/getting-started/runtime/).
//!
//! Create a Tokio Runtime:
//!
//! ```rust
//! # extern crate tokio;
//! # fn main () {
//! use tokio::runtime::Runtime;
//!
//! let mut rt = Runtime::new().unwrap();
//! # }
//! ```
//!
//! Subsequently, use the docker API:
//!
//! ```rust
//! # extern crate bollard;
//! # extern crate yup_hyper_mock;
//! # fn main () {
//! # use bollard::Docker;
//! # use bollard::image::ListImagesOptions;
//! # use yup_hyper_mock::SequentialConnector;
//! // Use a connection function described above
//! // let docker = Docker::connect_...;
//! # let mut connector = SequentialConnector::default();
//! # let docker = Docker::connect_with(connector, "localhost".to_string(), 5).unwrap();
//! let future = docker.list_images(None::<ListImagesOptions<String>>);
//! # }
//! ```
//!
//! Execute the future aynchronously:
//!
//! ```rust
//! # extern crate bollard;
//! # extern crate tokio;
//! # extern crate yup_hyper_mock;
//! # fn main () {
//! # use bollard::Docker;
//! # use bollard::image::ListImagesOptions;
//! # use tokio::runtime::Runtime;
//! # use tokio::prelude::Future;
//! # use yup_hyper_mock::SequentialConnector;
//! # let mut rt = Runtime::new().unwrap();
//! # let mut connector = SequentialConnector::default();
//! # let docker = Docker::connect_with(connector, "localhost".to_string(), 5).unwrap();
//! # let future = docker.list_images(None::<ListImagesOptions<String>>).map(|_| ()).map_err(|_| ());
//! rt.spawn(future);
//! # }
//! ```
//!
//! Or, to execute and receive the result:
//!
//! ```rust
//! # extern crate bollard;
//! # extern crate tokio;
//! # extern crate yup_hyper_mock;
//! # fn main () {
//! # use bollard::Docker;
//! # use bollard::image::ListImagesOptions;
//! # use tokio::runtime::Runtime;
//! # use tokio::prelude::Future;
//! # use yup_hyper_mock::SequentialConnector;
//! # let mut rt = Runtime::new().unwrap();
//! # let mut connector = SequentialConnector::default();
//! # connector.content.push(
//! #   "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".to_string()
//! # );
//! # let docker = Docker::connect_with(connector, "localhost".to_string(), 5).unwrap();
//! # let future = docker.list_images(None::<ListImagesOptions<String>>).map(|_| ()).map_err(|_| ());
//! let result = rt.block_on(future);
//! # }
//! ```
//!
//! Finally, to shut down the executor:
//!
//! ```rust
//! # extern crate tokio;
//! # fn main () {
//! # use tokio::runtime::Runtime;
//! # use tokio::prelude::Future;
//! # let mut rt = Runtime::new().unwrap();
//! rt.shutdown_now().wait().unwrap();
//! # }
//! ```
//!
//! # History
//!
//! This library stems from the [boondock rust library](https://github.com/faradayio/boondock),
//! which in turn originates from the [rust-docker library](https://github.com/ghmlee/rust-docker), but
//! most parts were rewritten to adobt the new functionality provided by tokio. Many thanks to the
//! original authors for the initial code and inspiration.
//!
//! # Integration tests
//!
//! Running the integration tests by default requires a running docker registry, with images tagged
//! and pushed there. To disable this behaviour, set the `DISABLE_REGISTRY` environment variable.
//!
//! ```bash
//! docker run -d --restart always --name registry -p 5000:5000 registry:2
//! docker pull hello-world:linux
//! docker pull fnichol/uhttpd
//! docker tag hello-world:linux localhost:5000/hello-world:linux
//! docker tag fnichol/uhttpd localhost:5000/fnichol/uhttpd
//! docker push localhost:5000/hello-world:linux
//! docker push localhost:5000/fnichol/uhttpd
//! REGISTRY_HTTP_ADDR=localhost:5000 cargo test --test-threads 1
//! ```

#[macro_use]
extern crate failure;
extern crate futures;
extern crate http;
extern crate hyper;
extern crate hyper_tls;
extern crate native_tls;
#[cfg(feature = "openssl")]
extern crate openssl;
#[macro_use]
extern crate serde_derive;
extern crate arrayvec;
extern crate bytes;
extern crate chrono;
extern crate dirs;
extern crate hex;
#[cfg(feature = "openssl")]
extern crate hyper_openssl;
#[cfg(unix)]
extern crate hyperlocal;
#[macro_use]
extern crate log;
extern crate mio;
#[cfg(windows)]
extern crate mio_named_pipes;
extern crate serde;
extern crate serde_json;
extern crate tokio;
extern crate tokio_codec;
extern crate tokio_io;
#[cfg(windows)]
extern crate tokio_reactor;
extern crate tokio_timer;
extern crate url;
#[cfg(windows)]
extern crate winapi;

// declare modules
pub mod auth;
pub mod container;
mod docker;
mod either;
pub mod errors;
pub mod image;
mod named_pipe;
mod options;
mod read;
pub mod system;
mod uri;

#[cfg(test)]
extern crate yup_hyper_mock as hyper_mock;

// publicly re-export
pub use docker::{Docker, DockerChain};
