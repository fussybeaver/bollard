//! Stream stats for all running Docker containers asynchronously
#![type_length_limit = "2097152"]

use bollard::container::{ListContainersOptions, StatsOptions};
use bollard::Docker;

use futures_util::stream::StreamExt;
use futures_util::stream::TryStreamExt;

use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let docker = Docker::connect_with_unix_defaults().unwrap();

    loop {
        let mut filter = HashMap::new();
        filter.insert(String::from("status"), vec![String::from("running")]);
        let containers = &docker
            .list_containers(Some(ListContainersOptions {
                all: true,
                filters: filter,
                ..Default::default()
            }))
            .await?;

        if containers.is_empty() {
            panic!("no running containers");
        } else {
            for container in containers {
                let container_id = container.id.as_ref().unwrap();
                let stream = &mut docker
                    .stats(container_id, Some(StatsOptions { stream: false }))
                    .take(1);

                while let Some(Ok(stats)) = stream.next().await {
                    println!(
                        "{} - {:?}: {:?} {:?}",
                        container_id, &container.names, container.image, stats
                    );
                }
            }
        }
    }
}
