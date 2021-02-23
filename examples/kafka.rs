//! This example will spin up Zookeeper and two Kafka brokers asynchronously.

use bollard::container::{Config, CreateContainerOptions, LogsOptions, StartContainerOptions};
use bollard::image::CreateImageOptions;
use bollard::models::*;
use bollard::Docker;

use futures_util::stream::select;
use futures_util::stream::StreamExt;
use futures_util::stream::TryStreamExt;

const KAFKA_IMAGE: &'static str = "confluentinc/cp-kafka:5.0.1";
const ZOOKEEPER_IMAGE: &'static str = "confluentinc/cp-zookeeper:5.0.1";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    let docker = Docker::connect_with_unix_defaults().unwrap();

    let sd1 = docker.clone();
    let sd2 = docker.clone();

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
        cmd: Some(vec!["/etc/confluent/docker/run"]),
        env: Some(vec![
            "KAFKA_ZOOKEEPER_CONNECT=localhost:32181",
            "KAFKA_ADVERTISED_LISTENERS=PLAINTEXT://localhost:19092",
            "KAFKA_BROKER_ID=1",
            "KAFKA_OFFSETS_TOPIC_REPLICATION_FACTOR=1",
        ]),
        host_config: Some(HostConfig {
            network_mode: Some(String::from("container:zookeeper")),
            ..Default::default()
        }),
        ..Default::default()
    };

    let broker2_config = Config {
        image: Some(KAFKA_IMAGE),
        cmd: Some(vec!["/etc/confluent/docker/run"]),
        env: Some(vec![
            "KAFKA_ZOOKEEPER_CONNECT=localhost:32181",
            "KAFKA_ADVERTISED_LISTENERS=PLAINTEXT://localhost:29092",
            "KAFKA_BROKER_ID=2",
            "KAFKA_OFFSETS_TOPIC_REPLICATION_FACTOR=1",
        ]),
        host_config: Some(HostConfig {
            network_mode: Some(String::from("container:zookeeper")),
            ..Default::default()
        }),
        ..Default::default()
    };

    &docker
        .create_image(
            Some(CreateImageOptions {
                from_image: ZOOKEEPER_IMAGE,
                ..Default::default()
            }),
            None,
            None,
        )
        .try_collect::<Vec<_>>()
        .await?;

    &docker
        .create_container(
            Some(CreateContainerOptions { name: "zookeeper" }),
            zookeeper_config,
        )
        .await?;

    &docker
        .start_container("zookeeper", None::<StartContainerOptions<String>>)
        .await?;

    &docker
        .create_image(
            Some(CreateImageOptions {
                from_image: KAFKA_IMAGE,
                ..Default::default()
            }),
            None,
            None,
        )
        .try_collect::<Vec<_>>()
        .await?;

    &docker
        .create_container(
            Some(CreateContainerOptions { name: "kafka1" }),
            broker1_config,
        )
        .await?;

    &docker
        .start_container("kafka1", None::<StartContainerOptions<String>>)
        .await?;

    let mut stream1 = sd1.logs::<String>(
        "kafka1",
        Some(LogsOptions {
            follow: true,
            stdout: true,
            stderr: false,
            ..Default::default()
        }),
    );

    &docker
        .create_image(
            Some(CreateImageOptions {
                from_image: KAFKA_IMAGE,
                ..Default::default()
            }),
            None,
            None,
        )
        .try_collect::<Vec<_>>()
        .await?;

    &docker
        .create_container(
            Some(CreateContainerOptions { name: "kafka2" }),
            broker2_config,
        )
        .await?;

    &docker
        .start_container("kafka2", None::<StartContainerOptions<String>>)
        .await?;

    let mut stream2 = sd2.logs::<String>(
        "kafka2",
        Some(LogsOptions {
            follow: true,
            stdout: true,
            stderr: false,
            ..Default::default()
        }),
    );

    let mut stream = select(&mut stream1, &mut stream2);

    while let Some(msg) = stream.next().await {
        println!("Message: {:?}", msg);
    }

    Ok(())
}
