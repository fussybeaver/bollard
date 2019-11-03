//! Stream stats for all running Docker containers asynchronously
#![type_length_limit = "2097152"]
#[macro_use]
extern crate failure;

use bollard::container::{ListContainersOptions, StatsOptions};
use bollard::Docker;
use failure::Error;

use futures_util::stream;
use futures_util::try_stream::TryStreamExt;
use tokio::prelude::*;
use tokio::runtime::Runtime;

use std::collections::HashMap;

async fn loop_fn<'a>(
    client: &bollard::Docker,
) -> Result<(&bollard::Docker, HashMap<String, String>), Error> {
    let mut filter = HashMap::new();
    filter.insert(String::from("status"), vec![String::from("running")]);
    let containers = client
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
                Ok::<_, failure::Error>((client, HashMap::new())),
                |xs, x| {
                    async move {
                        match xs {
                            Ok((client, mut hsh)) => {
                                client
                                    .stats(&x.0, Some(StatsOptions { stream: false }))
                                    .take(1)
                                    .map(|value| match value {
                                        Ok(stats) => {
                                            hsh.insert(
                                                String::from(&x.0),
                                                format!("{:?}: {} {:?}", &x.1, &x.2, stats),
                                            );
                                            Ok(())
                                        }
                                        Err(e) => {
                                            return Err(e);
                                        }
                                    })
                                    .try_collect::<Vec<()>>()
                                    .await?;

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

    let mut client = &docker;
    loop {
        match loop_fn(&client).await? {
            (c, h) => {
                println!("{:?}", &h);
                client = c;
            }
        }
    }
}

fn main() {
    env_logger::init();

    let rt = Runtime::new().unwrap();
    rt.block_on(run()).unwrap();
}
