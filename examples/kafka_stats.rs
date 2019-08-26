//! This example will spin up Zookeeper and three Kafka brokers asynchronously.

extern crate bollard;
extern crate failure;
extern crate futures;
extern crate hyper;
extern crate serde;
extern crate tokio;

use bollard::container::{
    Config, CreateContainerOptions, HostConfig, StartContainerOptions, Stats, StatsOptions,
};
use bollard::image::CreateImageOptions;
use bollard::{Docker, DockerChain};

use failure::Error;
use futures::stream::Stream;
use serde::ser::Serialize;
use tokio::prelude::*;
use tokio::runtime::Runtime;

use std::cmp::Eq;
use std::hash::Hash;

const KAFKA_IMAGE: &'static str = "confluentinc/cp-kafka:5.0.1";
const ZOOKEEPER_IMAGE: &'static str = "confluentinc/cp-zookeeper:5.0.1";

fn create_and_stats<T>(
    docker: DockerChain,
    name: &'static str,
    config: Config<T>,
) -> impl Stream<Item = Stats, Error = Error>
where
    T: AsRef<str> + Eq + Hash + Serialize,
{
    docker
        .create_container(Some(CreateContainerOptions { name: name }), config)
        .and_then(move |(docker, _)| {
            docker.start_container(name, None::<StartContainerOptions<String>>)
        })
        .and_then(move |(docker, _)| docker.stats(name, Some(StatsOptions { stream: true })))
        .map(|(_, stream)| stream)
        .into_stream()
        .flatten()
}

fn main() {
    let mut rt = Runtime::new().unwrap();
    #[cfg(unix)]
    let docker1 = Docker::connect_with_unix_defaults().unwrap();
    #[cfg(windows)]
    let docker1 = Docker::connect_with_named_pipe_defaults().unwrap();
    let docker2 = docker1.clone();
    let docker3 = docker1.clone();
    let docker4 = docker1.clone();

    let zookeeper_config = Config {
        image: Some(ZOOKEEPER_IMAGE),
        env: Some(vec![
            "ZOOKEEPER_CLIENT_PORT=32181",
            "ZOOKEEPER_TICK_TIME=2000",
            "ZOOKEEPER_SYNC_LIMIT=2",
        ]),
        ..Default::default()
    };

    let broker1_config = broker_config(BrokerId::One);
    let broker2_config = broker_config(BrokerId::Two);
    let broker3_config = broker_config(BrokerId::Three);

    let stream = docker1
        .chain()
        .create_image(
            Some(CreateImageOptions {
                from_image: ZOOKEEPER_IMAGE,
                ..Default::default()
            }),
            None,
        )
        .and_then(move |(docker, _)| {
            docker.create_container(
                Some(CreateContainerOptions { name: "zookeeper" }),
                zookeeper_config,
            )
        })
        .and_then(move |(docker, _)| {
            docker.start_container("zookeeper", None::<StartContainerOptions<String>>)
        })
        .map(move |(docker, _)| {
            let stream1 = docker
                .create_image(
                    Some(CreateImageOptions {
                        from_image: KAFKA_IMAGE,
                        ..Default::default()
                    }),
                    None,
                )
                .map(move |(docker, _)| create_and_stats(docker, "kafka1", broker1_config))
                .into_stream()
                .flatten();

            let stream2 = docker2
                .chain()
                .create_image(
                    Some(CreateImageOptions {
                        from_image: KAFKA_IMAGE,
                        ..Default::default()
                    }),
                    None,
                )
                .map(move |(docker, _)| create_and_stats(docker, "kafka2", broker2_config))
                .into_stream()
                .flatten();

            let stream3 = docker3
                .chain()
                .create_image(
                    Some(CreateImageOptions {
                        from_image: KAFKA_IMAGE,
                        ..Default::default()
                    }),
                    None,
                )
                .map(move |(docker, _)| create_and_stats(docker, "kafka3", broker3_config))
                .into_stream()
                .flatten();

            let stream4 = docker4.stats("zookeeper", Some(StatsOptions { stream: true }));

            stream1
                .zip(stream2)
                .zip(stream3)
                .zip(stream4)
                .map(|(((s1, s2), s3), s4)| vec![s1, s2, s3, s4])
        })
        .into_stream()
        .flatten();

    let future = stream.map_err(|e| println!("{:?}", e)).for_each(|lst| {
        // Erase Screen
        print!("\u{001b}c");
        println!("id\t\tname\t\tRSS\t\tmem %\t\ttime");
        lst.iter()
            .map(|x| {
                (
                    Some(shorten_string(&x.id)),
                    Some(shorten_string(&x.name)),
                    memory_resident_size(x),
                    memory_usage(x),
                    Some(x.read.format("%H:%M:%S")),
                )
            })
            .for_each(|tpl| match tpl {
                (Some(id), Some(name), Some(rss), Some(usage), Some(time)) => println!(
                    "{:<8}\t{:<8}\t{:.2}M\t\t{:.2}%\t\t{}",
                    id, name, rss, usage, time
                ),
                _ => (),
            });
        Ok(())
    });

    rt.spawn(future);

    rt.shutdown_on_idle().wait().unwrap();
}

enum BrokerId {
    One,
    Two,
    Three,
}

fn broker_config(id: BrokerId) -> Config<&'static str> {
    let (broker_id, advertised_listener) = match id {
        BrokerId::One => (
            "KAFKA_BROKER_ID=1",
            "KAFKA_ADVERTISED_LISTENERS=PLAINTEXT://localhost:19092",
        ),
        BrokerId::Two => (
            "KAFKA_BROKER_ID=2",
            "KAFKA_ADVERTISED_LISTENERS=PLAINTEXT://localhost:29092",
        ),
        BrokerId::Three => (
            "KAFKA_BROKER_ID=3",
            "KAFKA_ADVERTISED_LISTENERS=PLAINTEXT://localhost:39092",
        ),
    };
    Config {
        image: Some(KAFKA_IMAGE),
        cmd: Some(vec!["/etc/confluent/docker/run"]),
        env: Some(vec![
            "KAFKA_ZOOKEEPER_CONNECT=localhost:32181",
            advertised_listener,
            broker_id,
            "KAFKA_OFFSETS_TOPIC_REPLICATION_FACTOR=3",
        ]),
        host_config: Some(HostConfig {
            network_mode: Some("container:zookeeper"),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn shorten_string(s: &str) -> &str {
    if s.len() < 9 {
        s
    } else {
        &s[..8]
    }
}

fn memory_resident_size(stat: &bollard::container::Stats) -> Option<f64> {
    if let Some(s) = stat.memory_stats.stats {
        Some(s.rss as f64 / 1024. / 1024.)
    } else {
        None
    }
}

fn memory_usage(stat: &bollard::container::Stats) -> Option<f64> {
    if let (Some(usage), Some(limit)) = (stat.memory_stats.usage, stat.memory_stats.limit) {
        Some(100. * usage as f64 / limit as f64)
    } else {
        None
    }
}
