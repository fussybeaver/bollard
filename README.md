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
bollard = "0.8"
```

## API
### Documentation

[API docs](https://docs.rs/bollard/).

As of version 0.6, this project now generates API stubs from the upstream Docker-maintained
[Swagger OpenAPI specification](https://docs.docker.com/engine/api/v1.40.yaml). The generated
models are committed to this repository, but packaged in a separate crate
[bollard-stubs](https://crates.io/crates/bollard-stubs).

### Version

The [Docker API](https://docs.docker.com/engine/api/v1.40/) is pegged at version `1.40`. The
library also supports [version
negotiation](https://docs.rs/bollard/latest/bollard/struct.Docker.html#method.negotiate_version),
to allow downgrading to an older API version.

### Mapping Docker Client to Docker Engine API

The [Docker Engine API](https://docs.docker.com/engine/api/v1.40/) provides a
low-level interface to the Docker server, but Docker users may be more familiar
with the [Docker command
line](https://docs.docker.com/engine/reference/commandline/cli/) interface.

Bollard's API is aligned with the underlying docker engine API, not the CLI, so
understanding how to map between the two can be useful. This mapping can be
accomplished with the following command:

```bash
$ strace -fe trace=open,read,write -v -s 2048 -- <Docker CLI Command>

# Example:
$ strace -fe trace=open,read,write -v -s 2048 -- docker run --rm hello-world
```

Requests are made as HTTP requests, with a visible endpoint, such as:

```
write(3, "POST /v1.40/containers/create ... (args to Docker server)")
```

## Usage

### Connecting with the docker daemon

Connect to the docker server according to your architecture and security remit.

#### Unix socket

The client will connect to the standard unix socket location `/var/run/docker.sock`. Use the
`Docker::connect_with_unix` method API to parameterise the interface.

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

#### SSL via Rustls

The client will connect to the location pointed to by `DOCKER_HOST` environment variable, or
`localhost:2375` if missing.

The location pointed to by the `DOCKER_CERT_PATH` environment variable is searched for
certificates - `key.pem` for the private key, `cert.pem` for the server certificate and
`ca.pem` for the certificate authority chain.

Use the `Docker::connect_with_ssl` method API
to parameterise the interface.

```rust
use bollard::Docker;
#[cfg(feature = "ssl")]
Docker::connect_with_ssl_defaults();
```

### Examples

Note: all these examples need a [Tokio
Runtime](https://tokio.rs/).

#### Version

First, check that the API is working with your server:

```rust
use bollard::Docker;

use futures_util::future::FutureExt;

// Use a connection function described above
// let docker = Docker::connect_...;

async move {
    let version = docker.version().await.unwrap();
    println!("{:?}", version);
};
```

#### Listing images

To list docker images available on the Docker server:

```rust
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
```

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

Further examples are available in the [examples
folder](https://github.com/fussybeaver/bollard/tree/master/examples), or the [integration/unit
tests](https://github.com/fussybeaver/bollard/tree/master/tests).

## Development

Contributions are welcome, please observe the following advice.

## Building the stubs

Serialization stubs are generated through the [Swagger
library](https://github.com/swagger-api/swagger-codegen/). To generate these files, use the
following in the `codegen` folder:

```bash
mvn -D org.slf4j.simpleLogger.defaultLogLevel=debug clean compiler:compile generate-resources
```

## History

This library was originally forked from the [boondock rust
library](https://github.com/faradayio/boondock).  Many thanks to the original authors for the
initial code and inspiration.

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
