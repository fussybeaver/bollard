//! Stream stats for all running Docker containers asynchronously
extern crate bollard;
#[macro_use]
extern crate failure;
extern crate env_logger;
extern crate futures;
extern crate hyper;
extern crate serde;
extern crate tokio;

use bollard::container::{ListContainersOptions, StatsOptions};
use bollard::Docker;

use futures::future::{loop_fn, Loop};
use tokio::prelude::*;
use tokio::runtime::Runtime;

use std::collections::HashMap;

fn main() {
    env_logger::init();

    let mut rt = Runtime::new().unwrap();
    #[cfg(unix)]
    let docker = Docker::connect_with_unix_defaults().unwrap();
    #[cfg(windows)]
    let docker = Docker::connect_with_named_pipe_defaults().unwrap();

    let future = loop_fn(
        (docker.chain(), HashMap::new()),
        move |(client, monitor)| {
            let mut filter = HashMap::<&'static str, Vec<&'static str>>::new();
            filter.insert("status", vec!["running"]);
            client
                .list_containers(Some(ListContainersOptions {
                    all: true,
                    filters: filter,
                    ..Default::default()
                }))
                .and_then(move |(docker, containers)| {
                    if containers.is_empty() {
                        Err(bail!("no running containers"))
                    } else {
                        for c in containers
                            .iter()
                            .filter(|c| !monitor.contains_key(&c.id))
                            .map(|c| (c.id.to_owned(), c.names.to_owned(), c.image.to_owned()))
                        {
                            println!("Starting tokio spawn for container {}", &c.0);
                            tokio::executor::spawn(future::lazy(move || {
                                #[cfg(unix)]
                                let client = Docker::connect_with_unix_defaults().unwrap();
                                #[cfg(windows)]
                                let client = Docker::connect_with_named_pipe_defaults().unwrap();
                                client
                                    .stats(&c.0, Some(StatsOptions { stream: true }))
                                    .for_each(move |s| Ok(println!("{:?}:{} {:?}", c.1, c.2, s)))
                                    .map_err(|e| println!("{:?}", e))
                            }));
                        }

                        let mut new_monitor = HashMap::new();
                        for c in containers.iter().map(|c| c.id.to_owned()) {
                            new_monitor.insert(c, ());
                        }
                        Ok(Loop::Continue((docker, new_monitor)))
                    }
                })
                .map_err(|e| println!("{:?}", e))
        },
    );

    rt.spawn(future);

    rt.shutdown_on_idle().wait().unwrap();
}
