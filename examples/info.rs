//! Fetch info of all running containers concurrently

use bollard::container::{InspectContainerOptions, ListContainersOptions};
use bollard::models::ContainerSummary;
use bollard::Docker;

use std::collections::HashMap;
use std::default::Default;

use futures_util::stream;
use futures_util::stream::StreamExt;

async fn conc(arg: (Docker, &ContainerSummary)) -> () {
    let (docker, container) = arg;
    println!(
        "{:?}",
        docker
            .inspect_container(
                container.id.as_ref().unwrap(),
                None::<InspectContainerOptions>
            )
            .await
            .unwrap()
    )
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    #[cfg(unix)]
    let docker = Docker::connect_with_unix_defaults()?;
    #[cfg(windows)]
    let docker = Docker::connect_with_named_pipe_defaults()?;

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
    docker_stream
        .zip(stream::iter(containers))
        .for_each_concurrent(2, conc)
        .await;

    Ok(())
}
