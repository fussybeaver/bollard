//! Run top asynchronously across several docker containers
use bollard::container::{ListContainersOptions, TopOptions};
use bollard::models::*;
use bollard::Docker;

use std::collections::HashMap;
use std::default::Default;

use futures_util::future::TryFutureExt;
use futures_util::stream::FuturesUnordered;
use futures_util::stream::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    let docker = Docker::connect_with_unix_defaults()?;

    let mut list_container_filters = HashMap::new();
    list_container_filters.insert("status", vec!["running"]);

    let containers = docker
        .list_containers(Some(ListContainersOptions {
            all: true,
            filters: list_container_filters,
            ..Default::default()
        }))
        .await?;

    let mut futures = FuturesUnordered::new();
    let iter = containers.iter();

    for container in iter {
        if let Some(ref name) = container.id {
            futures.push(
                docker
                    .top_processes(name, Some(TopOptions { ps_args: "aux" }))
                    .map_ok(move |result| (name.to_owned(), result)),
            )
        }
    }

    println!("                                                                \tPID\tUSER\tTIME\tCOMMAND");
    while let Some(Ok((
        name,
        ContainerTopResponse {
            processes: Some(p), ..
        },
    ))) = futures.next().await
    {
        if let Some(p) = p.get(0) {
            print!("{}", name);
            for mut v in p.to_vec() {
                if v.len() > 30 {
                    v.truncate(30);
                }
                print!("\t{}", v);
            }
            print!("\n");
        }
    }

    Ok(())
}
