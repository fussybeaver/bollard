extern crate bollard;
extern crate chrono;
extern crate futures;
extern crate hyper;
extern crate tokio;

use bollard::auth::DockerCredentials;
use bollard::image::*;
use bollard::system::*;
use bollard::Docker;

use futures::future;
use hyper::rt::{Future, Stream};
use tokio::runtime::Runtime;

#[macro_use]
pub mod common;
use common::*;

#[derive(Debug)]
enum Results {
    CreateImageResults(CreateImageResults),
    EventsResults(EventsResults),
}

fn events_test(docker: Docker) {
    let image = move || {
        if cfg!(windows) {
            format!("{}hello-world:nanoserver", registry_http_addr())
        } else {
            format!("{}hello-world:linux", registry_http_addr())
        }
    };
    let docker2 = docker.clone();

    let rt = Runtime::new().unwrap();
    let stream = docker.events(None::<EventsOptions<String>>);

    let stream2 = docker2.create_image(
        Some(CreateImageOptions {
            from_image: image(),
            ..Default::default()
        }),
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

    let future = stream
        .map(|events_results| Results::EventsResults(events_results))
        .select(stream2.map(|image_results| Results::CreateImageResults(image_results)))
        .skip_while(|value| match value {
            Results::EventsResults(_) => future::ok(false),
            _ => future::ok(true),
        })
        .take(1)
        .collect();

    let iter = rt
        .block_on_all(future)
        .or_else(|e| {
            println!("{:?}", e);
            Err(e)
        })
        .unwrap()
        .into_iter();

    println!("{}", iter.len());

    assert!(iter
        .map(|value| {
            println!("{:?}", value);
            value
        })
        .any(|value| match value {
            Results::EventsResults(EventsResults { type_: t, .. }) => true,
            _ => false,
        }));
}

#[test]
fn integration_test_events() {
    connect_to_docker_and_run!(events_test);
}
