//! Stream stats for all running Docker containers asynchronously
#![type_length_limit = "2097152"]

use bollard::container::{ListContainersOptions, StatsOptions};
use bollard::Docker;

use futures_util::stream::StreamExt;
use futures_util::stream::TryStreamExt;
use tokio::runtime::Runtime;

use std::collections::HashMap;

async fn run<'a>() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(unix)]
    let docker = Docker::connect_with_unix_defaults().unwrap();
    #[cfg(windows)]
    let docker = Docker::connect_with_named_pipe_defaults().unwrap();

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
                &docker
                    .stats(container_id, Some(StatsOptions { stream: false }))
                    .take(1)
                    .map(|value| match value {
                        Ok(stats) => {
                            println!(
                                "{} - {:?}: {:?} {:?}",
                                container_id, &container.names, container.image, stats
                            );
                            Ok(())
                        }
                        Err(e) => {
                            return Err(e);
                        }
                    })
                    .try_collect::<Vec<()>>()
                    .await?;
            }
        }
    }
}

fn main() {
    env_logger::init();

    let mut rt = Runtime::new().unwrap();
    rt.block_on(run()).unwrap();
}
