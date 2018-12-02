extern crate bollard;
extern crate failure;
extern crate futures;
extern crate hyper;
#[cfg(unix)]
extern crate hyperlocal;
extern crate tokio;

use hyper::client::connect::Connect;
use hyper::rt::Future;
use tokio::runtime::Runtime;

use bollard::image::*;
use bollard::Docker;

use std::default::Default;

#[macro_use]
pub mod common;
use common::*;

fn create_image_test<C>(docker: Docker<C>)
where
    C: Connect + Sync + 'static,
{
    let rt = Runtime::new().unwrap();
    let future = chain_create_image_hello_world(docker.chain());
    run_runtime(rt, future);
}

fn search_images_test<C>(docker: Docker<C>)
where
    C: Connect + Sync + 'static,
{
    let rt = Runtime::new().unwrap();
    let future = docker
        .chain()
        .search_images(SearchImagesOptions {
            term: "hello-world",
            ..Default::default()
        }).map(|(docker, result)| {
            assert!(
                result
                    .into_iter()
                    .any(|api_image| &api_image.name == "hello-world")
            );
            docker
        });

    run_runtime(rt, future);
}

fn inspect_image_test<C>(docker: Docker<C>)
where
    C: Connect + Sync + 'static,
{
    let image = move || {
        if cfg!(windows) {
            format!("{}hello-world:nanoserver", registry_http_addr())
        } else {
            format!("{}hello-world:linux", registry_http_addr())
        }
    };

    let rt = Runtime::new().unwrap();
    let future = chain_create_image_hello_world(docker.chain())
        .and_then(move |docker| docker.inspect_image(&image()))
        .map(move |(docker, result)| {
            assert!(
                result
                    .repo_tags
                    .into_iter()
                    .any(|repo_tag| repo_tag == image().to_string())
            );
            docker
        });

    run_runtime(rt, future);
}

fn list_images_test<C>(docker: Docker<C>)
where
    C: Connect + Sync + 'static,
{
    let image = move || {
        if cfg!(windows) {
            format!("{}hello-world:nanoserver", registry_http_addr())
        } else {
            format!("{}hello-world:linux", registry_http_addr())
        }
    };

    let rt = Runtime::new().unwrap();
    let future = chain_create_image_hello_world(docker.chain())
        .and_then(move |docker| {
            docker.list_images(Some(ListImagesOptions::<String> {
                all: true,
                ..Default::default()
            }))
        }).map(move |(docker, result)| {
            assert!(result.into_iter().any(|api_image| {
                api_image
                    .repo_tags
                    .unwrap_or(vec![String::new()])
                    .into_iter()
                    .any(|repo_tag| repo_tag == image().to_string())
            }));
            docker
        });

    run_runtime(rt, future);
}

fn image_history_test<C>(docker: Docker<C>)
where
    C: Connect + Sync + 'static,
{
    let image = move || {
        if cfg!(windows) {
            format!("{}hello-world:nanoserver", registry_http_addr())
        } else {
            format!("{}hello-world:linux", registry_http_addr())
        }
    };

    let rt = Runtime::new().unwrap();
    let future = chain_create_image_hello_world(docker.chain())
        .and_then(move |docker| docker.image_history(&image()))
        .map(move |(docker, result)| {
            assert!(result.into_iter().take(1).any(|history| {
                history
                    .tags
                    .unwrap_or(vec![String::new()])
                    .into_iter()
                    .any(|tag| tag == image().to_string())
            }));
            docker
        });

    run_runtime(rt, future);
}

fn prune_images_test<C>(docker: Docker<C>)
where
    C: Connect + Sync + 'static,
{
    rt_exec!(
        docker.prune_images(None::<PruneImagesOptions<String>>),
        |_| ()
    );
}

fn remove_image_test<C>(docker: Docker<C>)
where
    C: Connect + Sync + 'static,
{
    let image = move || {
        if cfg!(windows) {
            format!("{}hello-world:nanoserver", registry_http_addr())
        } else {
            format!("{}hello-world:linux", registry_http_addr())
        }
    };

    let rt = Runtime::new().unwrap();
    let future = chain_create_image_hello_world(docker.chain())
        .and_then(move |docker| {
            docker.remove_image(
                &image(),
                Some(RemoveImageOptions {
                    noprune: true,
                    ..Default::default()
                }),
            )
        }).map(move |(docker, result)| {
            assert!(result.into_iter().any(|s| match s {
                RemoveImageResults::RemoveImageUntagged { untagged } => untagged == image(),
                _ => false,
            }));
            docker
        });

    run_runtime(rt, future);
}

#[test]
fn integration_test_search_images() {
    connect_to_docker_and_run!(search_images_test);
}

#[test]
fn integration_test_create_image() {
    connect_to_docker_and_run!(create_image_test);
}

#[test]
fn integration_test_inspect_image() {
    connect_to_docker_and_run!(inspect_image_test);
}

#[test]
fn integration_test_image_create() {
    connect_to_docker_and_run!(list_images_test);
}

#[test]
fn integration_test_image_history() {
    connect_to_docker_and_run!(image_history_test);
}

#[test]
fn integration_test_prune_images() {
    connect_to_docker_and_run!(prune_images_test);
}

#[test]
fn integration_test_remove_image() {
    connect_to_docker_and_run!(remove_image_test);
}
