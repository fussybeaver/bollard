# Docker

[![Build Status](https://travis-ci.org/ghmlee/rust-docker.svg)](https://travis-ci.org/ghmlee/rust-docker)

This is a Docker Remote API binding in Rust. Documentation is available [here](https://ghmlee.github.io/rust-docker/doc/docker).

## Quick start

```
[dependencies]
docker = "0.0.32"
```

```rust
extern crate docker;

use docker::Docker;

fn main() {
    let docker = match Docker::connect("unix:///var/run/docker.sock") {
    	Ok(docker) => docker,
        Err(e) => { panic!("{}", e); }
    };
}
```

## Debug
* Rust (>= v1.0.0-beta)
* Docker (>= v1.5.0)

## Examples

### Containers

```rust
extern crate docker;

use docker::Docker;

fn main() {
    let docker = match Docker::connect("unix:///var/run/docker.sock") {
    	Ok(docker) => docker,
        Err(e) => { panic!("{}", e); }
    };

    let containers = match docker.get_containers(false) {
        Ok(containers) => containers,
        Err(e) => { panic!("{}", e); }
    };
}
```

### Stats

```rust
extern crate docker;

use docker::Docker;

fn main() {
    let docker = match Docker::connect("unix:///var/run/docker.sock") {
    	Ok(docker) => docker,
        Err(e) => { panic!("{}", e); }
    };

    let containers = match docker.get_containers(false) {
        Ok(containers) => containers,
        Err(e) => { panic!("{}", e); }
    };

    let stats = match docker.get_stats(&containers[0]) {
        Ok(stats) => stats,
        Err(e) => { panic!("{}", e); }
    };
}
```

### Images

```rust
extern crate docker;

use docker::Docker;

fn main() {
    let docker = match Docker::connect("unix:///var/run/docker.sock") {
    	Ok(docker) => docker,
        Err(e) => { panic!("{}", e); }
    };

    let images = match docker.get_images(false) {
        Ok(images) => images,
        Err(e) => { panic!({}, e); }
    };
}

```

### Info

```rust
extern crate docker;

use docker::Docker;

fn main() {
    let docker = match Docker::connect("unix:///var/run/docker.sock") {
    	Ok(docker) => docker,
        Err(e) => { panic!("{}", e); }
    };

    let info = match docker.get_info() {
        Ok(info) => info,
        Err(e) => { panic!("{}", e); }
    };
}
```

## Boot2Docker

By default, `boot2docker` runs `docker` with TLS enabled. It auto-generates certificates and stores them in `/home/docker/.docker` inside the VM. The `boot2docker` up command will copy them to `~/.boot2docker/certs` on the host machine once the VM has started, and output the correct values for the `DOCKER_CERT_PATH` and `DOCKER_TLS_VERIFY` environment variables.

### Example

```rust
extern crate docker;

use docker::Docker;
use std::path::Path;

fn main() {
    let key = Path::new("/Users/<username>/.boot2docker/certs/boot2docker-vm/key.pem");
    let cert = Path::new("/Users/<username>/.boot2docker/certs/boot2docker-vm/cert.pem");
    let ca = Path::new("/Users/<username>/.boot2docker/certs/boot2docker-vm/ca.pem");

    let mut docker = match Docker::connect("tcp://192.168.59.103:2376") {
    	Ok(docker) => docker,
        Err(e) => { panic!("{}", e); }
    };
    docker.set_tls(&key, &cert, &ca).unwrap();
}
```