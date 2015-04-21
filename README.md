# Docker

[![Build Status](https://travis-ci.org/ghmlee/rust-docker.svg)](https://travis-ci.org/ghmlee/rust-docker)

This is a Docker Remote API binding in Rust. Documentation is available [here](https://ghmlee.github.io/rust-docker/doc/docker).

## Quick start

```
[dependencies]
docker = "0.0.20"
```

```rust
extern crate docker;

use docker::Docker;

let docker = Docker::new();
```

## Debug
* Rust (>= v1.0.0-beta)
* Docker (>= v1.5.0)

## Examples

### Containers

```rust
extern crate docker;

use docker::Docker;

let docker = Docker::new();

let containers = match docker.get_containers(false) {
    Ok(containers) => containers,
    Err(e) => { panic!("{}", e); }
};
```

### Stats

```rust
extern crate docker;

use docker::Docker;

let docker = Docker::new();

let containers = match docker.get_containers(false) {
    Ok(containers) => containers,
    Err(e) => { panic!("{}", e); }
};

let stats = match docker.get_stats(&containers[0]) {
    Ok(stats) => stats,
    Err(e) => { panic!("{}", e); }
};
```

### Images

```rust
extern crate docker;

use docker::Docker;

let docker = Docker::new();

let images = match docker.get_images(false) {
    Ok(images) => images,
    Err(e) => { panic!({}, e); }
};

```

### Info

```rust
extern crate docker;

use docker::Docker;

let docker = Docker::new();

let info = match docker.get_info() {
    Ok(info) => info,
    Err(e) => { panic!("{}", e); }
};
```