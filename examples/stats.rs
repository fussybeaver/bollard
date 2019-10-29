//! Stream stats for all running Docker containers asynchronously
#![type_length_limit = "2097152"]
#[macro_use]
extern crate failure;

use bollard::container::{ListContainersOptions, StatsOptions};
use bollard::Docker;
use failure::Error;

use futures_util::stream;
use tokio::prelude::*;
use tokio::runtime::Runtime;

use std::collections::HashMap;

async fn loop_fn<'a>(
    client: bollard::DockerChain,
) -> Result<(bollard::DockerChain, HashMap<String, String>), Error> {
    let mut filter = HashMap::new();
    filter.insert(String::from("status"), vec![String::from("running")]);
    let (docker, containers) = client
        .list_containers(Some(ListContainersOptions {
            all: true,
            filters: filter,
            ..Default::default()
        }))
        .await?;

    if containers.is_empty() {
        Err(bail!("no running containers"))
    } else {
        stream::iter(containers.into_iter().map(|c| (c.id, c.names, c.image)))
            .fold(
                Ok::<_, failure::Error>((docker, HashMap::new())),
                |xs, x| {
                    async move {
                        match xs {
                            Ok((client, mut hsh)) => {
                                let (client, stream) = client
                                    .stats(&x.0, Some(StatsOptions { stream: false }))
                                    .await?;

                                match stream.into_future().await {
                                    (Some(Ok(stats)), _) => {
                                        hsh.insert(
                                            String::from(&x.0),
                                            format!("{:?}: {} {:?}", &x.1, &x.2, stats),
                                        );
                                    }
                                    (Some(Err(e)), _) => return Err(e.into()),
                                    _ => (),
                                };

                                Ok((client, hsh))
                            }
                            Err(e) => Err(e),
                        }
                    }
                },
            )
            .await
    }
}

async fn run<'a>() -> Result<HashMap<String, String>, Error> {
    #[cfg(unix)]
    let docker = Docker::connect_with_unix_defaults().unwrap();
    #[cfg(windows)]
    let docker = Docker::connect_with_named_pipe_defaults().unwrap();

    let mut chain = docker.chain();
    loop {
        match loop_fn(chain).await? {
            (c, h) => {
                println!("{:?}", &h);
                chain = c;
            }
        }
    }
}

fn main() {
    env_logger::init();

    let rt = Runtime::new().unwrap();
    rt.block_on(run()).unwrap();
}
