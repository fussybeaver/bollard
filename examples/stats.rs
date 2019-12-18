//! Stream stats for all running Docker containers asynchronously
#![type_length_limit = "2097152"]
#[macro_use]
extern crate failure;

use bollard::container::{ListContainersOptions, StatsOptions};
use bollard::Docker;
use failure::Error;

use futures_util::stream;
use futures_util::stream::StreamExt;
use futures_util::stream::TryStreamExt;
use tokio::prelude::*;
use tokio::runtime::Runtime;

use std::collections::HashMap;

async fn run<'a>() -> Result<(), Error> {
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
            return Err(bail!("no running containers"));
        } else {
            for container in containers {
                &docker
                    .stats(&container.id, Some(StatsOptions { stream: false }))
                    .take(1)
                    .map(|value| match value {
                        Ok(stats) => {
                            println!(
                                "{} - {:?}: {} {:?}",
                                &container.id, &container.names, &container.image, stats
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
