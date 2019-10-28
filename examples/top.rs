//! Run top asynchronously across several docker containers in parallel
use bollard::container::{APIContainers, ListContainersOptions, TopOptions, TopResult};
use bollard::errors::Error;
use bollard::{Docker, DockerChain};

use std::collections::HashMap;
use std::default::Default;

use futures::stream::futures_ordered;
use futures::stream::futures_unordered;
use futures::{future, Future, Stream};
use tokio::runtime::Runtime;

/// flatten exists on an iterator in nightly
fn flatten<T>(lst: Vec<Option<T>>) -> Vec<T> {
    let mut o = vec![];
    for el in lst {
        if let Some(v) = el {
            o.push(v);
        }
    }
    o
}

fn top_processes(
    client: DockerChain,
    container: &APIContainers,
) -> impl Future<Item = Option<(String, (DockerChain, TopResult))>, Error = Error> {
    let name = container.id.to_owned();
    client
        .top_processes(&container.id, Some(TopOptions { ps_args: "ef" }))
        .map(|result| Some((name, result)))
}

fn main() {
    let mut rt = Runtime::new().unwrap();
    #[cfg(unix)]
    let docker = Docker::connect_with_unix_defaults().unwrap();
    #[cfg(windows)]
    let docker = Docker::connect_with_named_pipe_defaults().unwrap();

    let mut list_container_filters = HashMap::new();
    list_container_filters.insert("status", vec!["running"]);

    let future = docker
        .chain()
        .list_containers(Some(ListContainersOptions {
            all: true,
            filters: list_container_filters,
            ..Default::default()
        }))
        .map(|(docker, containers)| {
            let chunks = containers.chunks(2);
            futures_ordered(chunks.map(|slice| {
                // These all use the same underlying hyper memory allocation.
                let client1 = docker.clone();
                let client2 = docker.clone();
                let client3 = docker.clone();
                let client4 = docker.clone();

                let iter = slice.iter().collect::<Vec<&APIContainers>>();

                futures_unordered(flatten(vec![
                    iter.get(0)
                        .map(move |container| top_processes(client1, container)),
                    iter.get(1)
                        .map(move |container| top_processes(client2, container)),
                    iter.get(2)
                        .map(move |container| top_processes(client3, container)),
                    iter.get(3)
                        .map(move |container| top_processes(client4, container)),
                ])).fold(HashMap::new(), |mut hashmap, opt| {
                    if let Some((name, (_, result))) = opt {
                        hashmap.insert(name, result.processes.get(0).unwrap().to_vec());
                    }
                    future::ok::<_, Error>(hashmap)
                })
            })).fold(HashMap::new(), |mut hashmap, hsh| {
                for (name, result) in hsh {
                    hashmap.insert(name, result);
                }
                future::ok::<_, Error>(hashmap)
            })
        })
        .flatten()
        .map_err(|e| println!("{:?}", e))
        .map(|hsh| {
            // Erase Screen
            print!("\x1B[1J");
            println!("                                                                \tPID\tUSER\tTIME\tCOMMAND");
            for (name, result) in hsh {
                print!("{}", name);
                for mut v in result {
                    if v.len() > 30 {
                        v.truncate(30);
                    }
                    print!("\t{}", v);
                }
                print!("\n");
            }
        });

    rt.spawn(future);

    rt.shutdown_on_idle().wait().unwrap();
}
