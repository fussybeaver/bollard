//! Fetch info of all running containers concurrently

use bollard_next::container::{InspectContainerOptions, ListContainersOptions};
use bollard_next::models::ContainerSummary;
use bollard_next::Docker;

use std::collections::HashMap;
use std::default::Default;

use futures_util::stream;
use futures_util::stream::StreamExt;

async fn conc(arg: (Docker, &ContainerSummary)) {
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
    let docker = Docker::connect_with_socket_defaults().unwrap();

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
