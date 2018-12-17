//! This example will spin up Zookeeper and two Kafka brokers asynchronously.

extern crate bollard;
extern crate failure;
extern crate futures;
extern crate hyper;
extern crate serde;
extern crate tokio;

use bollard::container::{
    Config, CreateContainerOptions, HostConfig, LogOutput, LogsOptions, StartContainerOptions,
};
use bollard::image::CreateImageOptions;
use bollard::{Docker, DockerChain};

use failure::Error;
use hyper::client::connect::Connect;
use serde::ser::Serialize;
use tokio::prelude::*;
use tokio::runtime::Runtime;

use std::cmp::Eq;
use std::hash::Hash;

const KAFKA_IMAGE: &'static str = "confluentinc/cp-kafka:5.0.1";
const ZOOKEEPER_IMAGE: &'static str = "confluentinc/cp-zookeeper:5.0.1";

fn create_and_logs<C, T>(
    docker: DockerChain<C>,
    name: &'static str,
    config: Config<T>,
) -> impl Stream<Item = LogOutput, Error = Error>
where
    C: Connect + 'static + Sync,
    T: AsRef<str> + Eq + Hash + Serialize,
{
    docker
        .create_container(Some(CreateContainerOptions { name: name }), config)
        .and_then(move |(docker, _)| {
            docker.start_container(name, None::<StartContainerOptions<String>>)
        }).and_then(move |(docker, _)| {
            docker.logs(
                name,
                Some(LogsOptions {
                    follow: true,
                    stdout: true,
                    stderr: false,
                    ..Default::default()
                }),
            )
        }).map(|(_, stream)| stream)
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

    let zookeeper_config = Config {
        image: Some(ZOOKEEPER_IMAGE),
        env: Some(vec![
            "ZOOKEEPER_CLIENT_PORT=32181",
            "ZOOKEEPER_TICK_TIME=2000",
            "ZOOKEEPER_SYNC_LIMIT=2",
        ]),
        ..Default::default()
    };

    let broker1_config = Config {
        image: Some(KAFKA_IMAGE),
        cmd: vec!["/etc/confluent/docker/run"],
        env: Some(vec![
            "KAFKA_ZOOKEEPER_CONNECT=localhost:32181",
            "KAFKA_ADVERTISED_LISTENERS=PLAINTEXT://localhost:19092",
            "KAFKA_BROKER_ID=1",
            "KAFKA_OFFSETS_TOPIC_REPLICATION_FACTOR=1",
        ]),
        host_config: Some(HostConfig {
            network_mode: Some("container:zookeeper"),
            ..Default::default()
        }),
        ..Default::default()
    };

    let broker2_config = Config {
        image: Some(KAFKA_IMAGE),
        cmd: vec!["/etc/confluent/docker/run"],
        env: Some(vec![
            "KAFKA_ZOOKEEPER_CONNECT=localhost:32181",
            "KAFKA_ADVERTISED_LISTENERS=PLAINTEXT://localhost:29092",
            "KAFKA_BROKER_ID=2",
            "KAFKA_OFFSETS_TOPIC_REPLICATION_FACTOR=1",
        ]),
        host_config: Some(HostConfig {
            network_mode: Some("container:zookeeper"),
            ..Default::default()
        }),
        ..Default::default()
    };

    let stream = docker1
        .chain()
        .create_image(Some(CreateImageOptions {
            from_image: ZOOKEEPER_IMAGE,
            ..Default::default()
        })).and_then(move |(docker, _)| {
            docker.create_container(
                Some(CreateContainerOptions { name: "zookeeper" }),
                zookeeper_config,
            )
        }).and_then(move |(docker, _)| {
            docker.start_container("zookeeper", None::<StartContainerOptions<String>>)
        }).map(|(docker, _)| {
            let stream1 = docker
                .create_image(Some(CreateImageOptions {
                    from_image: KAFKA_IMAGE,
                    ..Default::default()
                })).map(move |(docker, _)| create_and_logs(docker, "kafka1", broker1_config))
                .into_stream()
                .flatten();

            let stream2 = docker2
                .chain()
                .create_image(Some(CreateImageOptions {
                    from_image: KAFKA_IMAGE,
                    ..Default::default()
                })).map(move |(docker, _)| create_and_logs(docker, "kafka2", broker2_config))
                .into_stream()
                .flatten();

            stream1.select(stream2)
        }).into_stream()
        .flatten();

    let future = stream
        .map_err(|e| println!("{:?}", e))
        .for_each(|x| Ok(println!("{:?}", x)));

    rt.spawn(future);

    rt.shutdown_on_idle().wait().unwrap();
}
