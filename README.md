# Docker for Rust [![Build Status](https://travis-ci.org/ghmlee/rust-docker.svg)](https://travis-ci.org/ghmlee/rust-docker)

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