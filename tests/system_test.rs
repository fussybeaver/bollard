use bollard::auth::DockerCredentials;
use bollard::errors::Error;
use bollard::image::*;
use bollard::models::*;
use bollard::system::*;
use bollard::Docker;

use chrono::Utc;
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
    CreateImageResults(BuildInfo),
    EventsResults(SystemEventsResponse),
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
        stream.map_ok(|events_results| Results::EventsResults(events_results)),
        stream2.map_ok(|image_results| Results::CreateImageResults(image_results)),
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
        .map(|value| {
            println!("{:?}", value);
            value
        })
        .any(|value| match value {
            Results::EventsResults(SystemEventsResponse { _type: _, .. }) => true,
            _ => false,
        }));

    Ok(())
}

async fn events_until_forever_test(docker: Docker) -> Result<(), Error> {
    let image = if cfg!(windows) {
        format!("{}hello-world:nanoserver", registry_http_addr())
    } else {
        format!("{}hello-world:linux", registry_http_addr())
    };

    let start_time = Utc::now();

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
        stream.map_ok(|events_results| Results::EventsResults(events_results)),
        stream2.map_ok(|image_results| Results::CreateImageResults(image_results)),
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
        .map(|value| {
            println!("{:?}", value);
            value
        })
        .any(|value| match value {
            Results::EventsResults(SystemEventsResponse { _type: _, .. }) => true,
            _ => false,
        }));

    Ok(())
}
async fn df_test(docker: Docker) -> Result<(), Error> {
    create_image_hello_world(&docker).await?;

    let result = &docker.df().await?;

    let c: Vec<_> = result
        .images
        .as_ref()
        .unwrap()
        .iter()
        .filter(|c: &&ImageSummary| c.repo_tags.iter().any(|r| r.contains("hello-world")))
        .collect();

    assert!(c.len() > 0);

    Ok(())
}

#[test]
fn integration_test_events() {
    connect_to_docker_and_run!(events_test);
}

#[test]
fn integration_test_events_until_forever() {
    connect_to_docker_and_run!(events_until_forever_test);
}

#[test]
fn integration_test_df() {
    connect_to_docker_and_run!(df_test);
}
