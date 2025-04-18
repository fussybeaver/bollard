#![allow(deprecated)]
use bollard::auth::DockerCredentials;
use bollard::errors::Error;
use bollard::image::*;
use bollard::models::*;
use bollard::system::*;
use bollard::Docker;

use futures_util::future;
use futures_util::stream::select;
use futures_util::stream::StreamExt;
use futures_util::stream::TryStreamExt;
use tokio::runtime::Runtime;

#[macro_use]
pub mod common;
use common::*;

#[derive(Debug)]
enum Results {
    #[allow(dead_code)]
    CreateImageResults(CreateImageInfo),
    EventsResults(EventMessage),
}

async fn events_test(docker: Docker) -> Result<(), Error> {
    let image = if cfg!(windows) {
        format!("{}hello-world:nanoserver", registry_http_addr())
    } else {
        format!("{}hello-world:linux", registry_http_addr())
    };

    let stream = docker.events(None::<EventsOptions<String>>);

    let stream2 = docker.create_image(
        Some(CreateImageOptions {
            from_image: &image[..],
            ..Default::default()
        }),
        None,
        if cfg!(windows) {
            None
        } else {
            Some(DockerCredentials {
                username: Some("bollard".to_string()),
                password: std::env::var("REGISTRY_PASSWORD").ok(),
                ..Default::default()
            })
        },
    );

    let vec = select(
        stream.map_ok(Results::EventsResults),
        stream2.map_ok(Results::CreateImageResults),
    )
    .skip_while(|value| match value {
        Ok(Results::EventsResults(_)) => future::ready(false),
        _ => future::ready(true),
    })
    .take(1)
    .try_collect::<Vec<_>>()
    .await?;

    assert!(vec
        .iter()
        .inspect(|&value| {
            println!("{value:?}");
        })
        .any(|value| matches!(value, Results::EventsResults(EventMessage { typ: _, .. }))));

    Ok(())
}

#[cfg(any(feature = "time", feature = "chrono"))]
async fn events_until_forever_test(docker: Docker) -> Result<(), Error> {
    let image = if cfg!(windows) {
        format!("{}hello-world:nanoserver", registry_http_addr())
    } else {
        format!("{}hello-world:linux", registry_http_addr())
    };

    #[cfg(feature = "time")]
    let start_time = time::OffsetDateTime::now_utc();
    #[cfg(feature = "chrono")]
    let start_time = chrono::Utc::now();

    let stream = docker.events(Some(EventsOptions::<String> {
        since: Some(start_time),
        until: None,
        ..Default::default()
    }));

    let stream2 = docker.create_image(
        Some(CreateImageOptions {
            from_image: &image[..],
            ..Default::default()
        }),
        None,
        if cfg!(windows) {
            None
        } else {
            Some(DockerCredentials {
                username: Some("bollard".to_string()),
                password: std::env::var("REGISTRY_PASSWORD").ok(),
                ..Default::default()
            })
        },
    );

    let vec = select(
        stream.map_ok(Results::EventsResults),
        stream2.map_ok(Results::CreateImageResults),
    )
    .skip_while(|value| match value {
        Ok(Results::EventsResults(_)) => future::ready(false),
        _ => future::ready(true),
    })
    .take(1)
    .try_collect::<Vec<_>>()
    .await?;

    assert!(vec
        .iter()
        .any(|value| matches!(value, Results::EventsResults(EventMessage { typ: _, .. }))));

    Ok(())
}

#[cfg(not(feature = "test_macos"))]
async fn df_test(docker: Docker) -> Result<(), Error> {
    use bollard::query_parameters::DataUsageOptions;

    create_image_hello_world(&docker).await?;

    let result = &docker.df(None::<DataUsageOptions>).await?;

    let c = result
        .images
        .as_ref()
        .unwrap()
        .iter()
        .filter(|c: &&ImageSummary| c.repo_tags.iter().any(|r| r.contains("hello-world")));

    assert!(c.count() > 0);

    Ok(())
}

async fn info_test(docker: Docker) -> Result<(), Error> {
    let res = &docker.info().await?;
    let os_type = if cfg!(windows) { "windows" } else { "linux" };

    assert_eq!(os_type, res.os_type.as_ref().unwrap());

    Ok(())
}

async fn ping_test(docker: Docker) -> Result<(), Error> {
    let res = &docker.ping().await?;
    assert_eq!("OK", res);

    Ok(())
}

#[test]
fn integration_test_events() {
    connect_to_docker_and_run!(events_test);
}

#[test]
#[cfg(all(not(windows), any(feature = "chrono", feature = "time")))]
fn integration_test_events_until_forever() {
    connect_to_docker_and_run!(events_until_forever_test);
}

#[test]
#[cfg(not(feature = "test_macos"))]
fn integration_test_df() {
    connect_to_docker_and_run!(df_test);
}

#[test]
fn integration_test_info() {
    connect_to_docker_and_run!(info_test);
}

#[test]
fn integration_test_ping() {
    connect_to_docker_and_run!(ping_test);
}
