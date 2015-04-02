# Docker

[![Build Status](https://travis-ci.org/ghmlee/rust-docker.svg)](https://travis-ci.org/ghmlee/rust-docker)

Documentation is available [here](https://ghmlee.github.io/rust-docker/doc/docker).

## Docker
```rust
extern crate docker;

use docker::Docker;

let docker = Docker::new();
```

## GET /containers/json
```rust
let containers = docker.get_containers();
```