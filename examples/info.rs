//! Fetch info of all running containers in parallel
extern crate bollard;
extern crate failure;
extern crate futures;
extern crate hyper;
extern crate tokio;

use bollard::container::{InspectContainerOptions, ListContainersOptions};
use bollard::Docker;

use std::collections::HashMap;
use std::default::Default;

use futures::{stream, Future, Stream};
use tokio::runtime::Runtime;

fn main() {
    let mut rt = Runtime::new().unwrap();
    #[cfg(unix)]
    let docker = Docker::connect_with_unix_defaults().unwrap();
    #[cfg(windows)]
    let docker = Docker::connect_with_named_pipe_defaults().unwrap();

    let mut list_container_filters = HashMap::new();
    list_container_filters.insert("status", vec!["running"]);

    let future = docker
        .chain()
        .list_containers(Some(ListContainersOptions {
            all: true,
            filters: list_container_filters,
            ..Default::default()
        }))
        .into_stream()
        .map(|(docker, containers)| {
            let docker_stream = stream::repeat(docker);
            let container_stream = stream::iter_ok(containers);

            docker_stream.zip(container_stream)
        })
        .flatten()
        .and_then(|(docker, container)| {
            docker.inspect_container(&container.id, None::<InspectContainerOptions>)
        })
        .map(|(_, container)| {
            println!("{:?}", container);
        })
        .into_future()
        .map_err(|_| ())
        .map(|_| ());

    rt.spawn(future);

    rt.shutdown_on_idle().wait().unwrap();
}
