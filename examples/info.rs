//! Fetch info of all running containers concurrently

use bollard::container::{InspectContainerOptions, ListContainersOptions};
use bollard::Docker;

use std::collections::HashMap;
use std::default::Default;

use futures_util::stream;
use futures_util::stream::StreamExt;
use futures_util::try_stream::TryStreamExt;
use tokio::runtime::Runtime;

async fn run() -> Result<(), failure::Error> {
    #[cfg(unix)]
    let docker = Docker::connect_with_unix_defaults().unwrap();
    #[cfg(windows)]
    let docker = Docker::connect_with_named_pipe_defaults().unwrap();

    let mut list_container_filters = HashMap::new();
    list_container_filters.insert("status", vec!["running"]);

    let containers = &docker
        .list_containers(Some(ListContainersOptions {
            all: true,
            filters: list_container_filters,
            ..Default::default()
        }))
        .await?;

    let docker_stream = stream::repeat(docker);
    let container_stream = docker_stream
        .zip(stream::iter(containers))
        .for_each_concurrent(2, conc)
        .await;
    Ok(())
}

async fn conc(arg: (Docker, &bollard::container::APIContainers)) -> () {
    let (docker, container) = arg;
    println!(
        "{:?}",
        docker
            .inspect_container(&container.id, None::<InspectContainerOptions>)
            .await
            .unwrap()
    )
}

fn main() {
    env_logger::init();

    let rt = Runtime::new().unwrap();

    rt.block_on(run()).unwrap();
}
