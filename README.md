# Docker for Rust [![Build Status](https://travis-ci.org/ghmlee/rust-docker.svg)](https://travis-ci.org/ghmlee/rust-docker)

## Docker
```
extern crate docker;

use docker::Docker;

let docker = Docker::new();
```

## GET /containers/json
```
let containers = docker.get_containers();
```