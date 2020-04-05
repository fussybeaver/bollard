[![crates.io](https://img.shields.io/crates/v/bollard.svg)](https://crates.io/crates/bollard)
[![license](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![circle-ci](https://circleci.com/gh/fussybeaver/bollard/tree/master.svg?style=svg)](https://circleci.com/gh/fussybeaver/bollard/tree/master)
[![appveyor](https://ci.appveyor.com/api/projects/status/n5khebyfae0u1sbv/branch/master?svg=true)](https://ci.appveyor.com/project/fussybeaver/boondock)
[![docs](https://docs.rs/bollard/badge.svg)](https://docs.rs/bollard/)

## Bollard: an asynchronous rust client library for the docker API

Bollard leverages the latest [Hyper](https://github.com/hyperium/hyper) and
[Tokio](https://github.com/tokio-rs/tokio) improvements for an asynchronous API containing
futures, streams and the async/await paradigm.

The library also features Windows support through Named Pipes and HTTPS support through
optional SSL bindings or a native TLS implementation.

## Install

Add the following to your `Cargo.toml` file

```nocompile
[dependencies]
bollard = "0.4"
```

## API

### Documentation

[API docs](https://docs.rs/bollard/)

### Version

The [Docker API](https://docs.docker.com/engine/api/v1.40/) is pegged at version `1.40`

## Usage

### Connecting with the docker daemon

Connect to the docker server according to your architecture and security remit.

#### Unix socket

The client will connect to the standard unix socket location `/var/run/docker.sock`. Use the
`Docker::connect_with_unix` method API to parameterise the
interface.

```rust
use bollard::Docker;
#[cfg(unix)]
Docker::connect_with_unix_defaults();
```

#### Windows named pipe

The client will connect to the standard windows pipe location `\\.\pipe\docker_engine`. Use the
`Docker::connect_with_name_pipe` method API
to parameterise the interface.

```rust
use bollard::Docker;
#[cfg(windows)]
Docker::connect_with_named_pipe_defaults();
```

#### Local

The client will connect to the OS specific handler it is compiled for.
This is a convenience for localhost environment that should run on multiple
operating systems.
Use the `Docker::connect_with_local` method API to parameterise the interface.

```rust
use bollard::Docker;
Docker::connect_with_local_defaults();
```

#### HTTP

The client will connect to the location pointed to by `DOCKER_HOST` environment variable, or
`localhost:2375` if missing. Use the
`Docker::connect_with_http` method API to
parameterise the interface.

```rust
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

```rust
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

```rust
use bollard::Docker;
#[cfg(feature = "tls")]
Docker::connect_with_tls_defaults();
```

### Examples

Note: all these examples need a [Tokio
Runtime](https://tokio.rs/docs/getting-started/runtime/). A small example about how to use
Tokio is further below.

#### Version

First, check that the API is working with your server:

````rust, no_run
use bollard::Docker;

use futures_util::future::FutureExt;

// Use a connection function described above
// let docker = Docker::connect_...;
## let docker = Docker::connect_with_local_defaults().unwrap();

async move {
    let version = docker.version().await.unwrap();
    println!("{:?}", version);
};
```rust

### Listing images

To list docker images available on the Docker server:

```rust,no_run
use bollard::Docker;
use bollard::image::ListImagesOptions;

use futures_util::future::FutureExt;

use std::default::Default;

// Use a connection function described above
// let docker = Docker::connect_...;

async move {
    let images = &docker.list_images(Some(ListImagesOptions::<String> {
        all: true,
        ..Default::default()
    })).await.unwrap();

    for image in images {
        println!("-> {:?}", image);
    }
};
````

### Streaming Stats

To receive a stream of stats for a running container.

```rust
use bollard::Docker;
use bollard::container::StatsOptions;

use futures_util::stream::TryStreamExt;

use std::default::Default;

// Use a connection function described above
// let docker = Docker::connect_...;

async move {
    let stats = &docker.stats("postgres", Some(StatsOptions {
       stream: true,
       ..Default::default()
    })).try_collect::<Vec<_>>().await.unwrap();

    for stat in stats {
        println!("{} - mem total: {:?} | mem usage: {:?}",
            stat.name,
            stat.memory_stats.max_usage,
            stat.memory_stats.usage);
    }
};
```

## Examples

Further examples are available in the examples folder, or the integration/unit tests.

### A Primer on the Tokio Runtime

In order to use the API effectively, you will need to be familiar with the [Tokio
Runtime](https://tokio.rs/docs/getting-started/runtime/).

Create a Tokio Runtime:

```rust
use tokio::runtime::Runtime;

let rt = Runtime::new().unwrap();
```

Subsequently, use the docker API:

```rust
// Use a connection function described above
// let docker = Docker::connect_...;
let future = async move {
    &docker.list_images(None::<ListImagesOptions<String>>).await;
};
```

Execute the future aynchronously:

```rust
rt.spawn(future);
```

Or, to execute and receive the result:

```rust
let result = rt.block_on(future);
```

## History

This library stems from the [boondock rust library](https://github.com/faradayio/boondock),
which in turn originates from the [rust-docker library](https://github.com/ghmlee/rust-docker), but
most parts were rewritten to adobt the new functionality provided by tokio. Many thanks to the
original authors for the initial code and inspiration.

## Integration tests

Running the integration tests by default requires a running docker registry, with images tagged
and pushed there. To disable this behaviour, set the `DISABLE_REGISTRY` environment variable.

```bash
docker run -d --restart always --name registry -p 5000:5000 registry:2
docker pull hello-world:linux
docker pull fussybeaver/uhttpd
docker pull alpine
docker tag hello-world:linux localhost:5000/hello-world:linux
docker tag fussybeaver/uhttpd localhost:5000/fussybeaver/uhttpd
docker tag alpine localhost:5000/alpine
docker push localhost:5000/hello-world:linux
docker push localhost:5000/fussybeaver/uhttpd
docker push localhost:5000/alpine
docker swarm init
REGISTRY_HTTP_ADDR=localhost:5000 cargo test -- --test-threads 1
```

License: Apache-2.0
