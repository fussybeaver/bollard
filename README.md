# bollard

## Bollard: an asynchronous rust client library for the docker API

Bollard leverages the latest [Hyper](https://github.com/hyperium/hyper) and
[Tokio](https://github.com/tokio-rs/tokio) improvements for an asynchronous API containing
futures and streams.

The library also features Windows support through Named Pipes and HTTPS support through
optional SSL bindings or a native TLS implementation.

## Install

Add the following to your `Cargo.toml` file

```nocompile
[dependencies]
bollard = "0.1"
```

## Usage

### Connecting with the docker daemon

Connect to the docker server according to your architecture and security remit.

#### Unix socket

The client will connect to the standard unix socket location `/var/run/docker.sock`. Use the
`Docker::connect_with_unix` method API to parameterise the
interface.

```rust,norun
use bollard::Docker;
#[cfg(unix)]
Docker::connect_with_unix_defaults();
```

#### Windows named pipe

The client will connect to the standard windows pipe location `\\.\pipe\docker_engine`. Use the
`Docker::connect_with_name_pipe` method API
to parameterise the interface.

```rust,norun
use bollard::Docker;
#[cfg(windows)]
Docker::connect_with_named_pipe_defaults();
```

#### HTTP

The client will connect to the location pointed to by `DOCKER_HOST` environment variable, or
`localhost:2375` if missing. Use the
`Docker::connect_with_http` method API to
parameterise the interface.

```rust,norun
use bollard::Docker;
Docker::connect_with_http_defaults();
```

#### SSL via openssl

Openssl is switched off by default, and can be enabled through the `ssl` cargo feature.

The client will connect to the location pointed to by `DOCKER_HOST` environment variable, or
`localhost:2375` if missing.

The location pointed to by the `DOCKER_CERT_PATH` environment variable is searched for
certificates - `key.pem` for the private key, `cert.pem` for the server certificate and
`ca.pem` for the certificate authority chain.

Use the `Docker::connect_with_ssl` method API
to parameterise the interface.

```rust,norun
use bollard::Docker;
#[cfg(feature = "openssl")]
Docker::connect_with_ssl_defaults();
```

#### TLS

Native TLS allows you to avoid the SSL bindings.

The client will connect to the location pointed to by `DOCKER_HOST` environment variable, or
`localhost:2375` if missing.

The location pointed to by the `DOCKER_CERT_PATH` environment variable is searched for
certificates - `identity.pfx` for the PKCS #12 archive and `ca.pem` for the certificate
authority chain.

Use the `Docker::connect_with_ssl` method API
to parameterise the interface.

```rust,norun
use bollard::Docker;
Docker::connect_with_tls_defaults();
```

### Examples

Note: all these examples need a [Tokio
Runtime](https://tokio.rs/docs/getting-started/runtime/). A small example about how to use
Tokio is below.

#### Version

First, check that the API is working with your server:

```rust,norun
# extern crate bollard;
# extern crate futures;
# extern crate yup_hyper_mock;
# fn main () {
use futures::Future;

use bollard::Docker;

// Use a connection function described above
// let docker = Docker::connect_...;
# use yup_hyper_mock::SequentialConnector;
# let mut connector = SequentialConnector::default();
let docker = Docker::connect_with(connector, String::new(), 5).unwrap();

docker.version()
    .map(|version| {
        println!("{:?}", version);
    });
# }
```

#### Listing images

To list docker images available on the Docker server:

```rust,norun
# extern crate bollard;
# extern crate futures;
# extern crate yup_hyper_mock;
# fn main () {
use futures::Future;

use bollard::Docker;
use bollard::image::ListImagesOptions;

use std::default::Default;

// Use a connection function described above
// let docker = Docker::connect_...;
# use yup_hyper_mock::SequentialConnector;
# let mut connector = SequentialConnector::default();
let docker = Docker::connect_with(connector, String::new(), 5).unwrap();

docker.list_images(Some(ListImagesOptions::<String> {
   all: true,
   ..Default::default()
}))
  .map(|images| {
       for i in images {
           println!("-> {:?}", i);
       }
   });
# }
```

### Streaming Stats

To receive a stream of stats for a running container.

```rust,norun
# extern crate bollard;
# extern crate futures;
# extern crate yup_hyper_mock;
# fn main () {
use futures::stream::Stream;

use bollard::Docker;
use bollard::container::StatsOptions;

use std::default::Default;

// Use a connection function described above
// let docker = Docker::connect_...;
# use yup_hyper_mock::SequentialConnector;
# let mut connector = SequentialConnector::default();
let docker = Docker::connect_with(connector, String::new(), 5).unwrap();

docker.stats("postgres", Some(StatsOptions {
   stream: true,
   ..Default::default()
}))
  .map(|stat| {
        println!("{} - mem total: {:?} | mem usage: {:?}",
            stat.name,
            stat.memory_stats.max_usage,
            stat.memory_stats.usage);
   });
# }
```

### Chaining docker commands

It's sometimes more convenient to chain a string of Docker API calls. The `DockerChain` API
will return an instance of itself in the return call.

```rust,norun
# extern crate bollard;
# extern crate tokio;
# extern crate yup_hyper_mock;
# fn main () {
use bollard::Docker;
use bollard::image::CreateImageOptions;
use bollard::container::CreateContainerOptions;
use bollard::container::Config;

use tokio::prelude::Future;

use std::default::Default;
# use yup_hyper_mock::SequentialConnector;
# let mut connector = SequentialConnector::default();
# let docker = Docker::connect_with(connector, String::new(), 5).unwrap();
docker.chain().create_image(Some(CreateImageOptions{
    from_image: "hello-world",
    ..Default::default()
})).and_then(|(docker, _)|
    docker.create_container(
        None::<CreateContainerOptions<String>>,
        Config {
            image: Some("hello-world"),
            cmd: vec!["/hello"],
            ..Default::default()
        }));
# }
```

### A Primer on the Tokio Runtime

In order to use the API effectively, you will need to be familiar with the [Tokio
Runtime](https://tokio.rs/docs/getting-started/runtime/).

Create a Tokio Runtime:

```rust,norun
# extern crate tokio;
# fn main () {
use tokio::runtime::Runtime;

let mut rt = Runtime::new().unwrap();
# }
```

Subsequently, use the docker API:

```rust,norun
# extern crate bollard;
# extern crate yup_hyper_mock;
# fn main () {
# use bollard::Docker;
# use bollard::image::ListImagesOptions;
# use yup_hyper_mock::SequentialConnector;
// Use a connection function described above
// let docker = Docker::connect_...;
# let mut connector = SequentialConnector::default();
let docker = Docker::connect_with(connector, String::new(), 5).unwrap();
let future = docker.list_images(None::<ListImagesOptions<String>>);
# }
```

To execute the future aynchronously:

```rust,norun
# extern crate bollard;
# extern crate tokio;
# extern crate yup_hyper_mock;
# fn main () {
# use bollard::Docker;
# use bollard::image::ListImagesOptions;
# use tokio::runtime::Runtime;
# use tokio::prelude::Future;
# use yup_hyper_mock::SequentialConnector;
# let mut rt = Runtime::new().unwrap();
# let mut connector = SequentialConnector::default();
# let docker = Docker::connect_with(connector, String::new(), 5).unwrap();
# let future = docker.list_images(None::<ListImagesOptions<String>>).map(|_| ()).map_err(|_| ());
rt.spawn(future);
# }
```

Or, to execute and receive the result:

```rust,norun
# extern crate bollard;
# extern crate tokio;
# extern crate yup_hyper_mock;
# fn main () {
# use bollard::Docker;
# use bollard::image::ListImagesOptions;
# use tokio::runtime::Runtime;
# use tokio::prelude::Future;
# use yup_hyper_mock::SequentialConnector;
# let mut rt = Runtime::new().unwrap();
# let mut connector = SequentialConnector::default();
# connector.content.push(
#   "HTTP/1.1 200 OK\r\nServer: mock1\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n".to_string()
# );
# let docker = Docker::connect_with(connector, String::new(), 5).unwrap();
# let future = docker.list_images(None::<ListImagesOptions<String>>).map(|_| ()).map_err(|_| ());
let result = rt.block_on(future);
# }
```

Finally, to shut down the executor:

```rust, norun
## extern crate tokio;
## fn main () {
## use tokio::runtime::Runtime;
use tokio::prelude::Future;
## let mut rt = Runtime::new().unwrap();
rt.shutdown_now().wait().unwrap();
## }
```rust


License: Apache-2.0
