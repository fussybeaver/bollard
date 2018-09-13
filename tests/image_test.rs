extern crate boondock;
extern crate failure;
extern crate futures;
extern crate hyper;
#[cfg(unix)]
extern crate hyperlocal;
extern crate tokio;

use failure::Error;
use futures::future;
use hyper::client::connect::Connect;
use hyper::rt::{Future, Stream};
use tokio::runtime::Runtime;

use boondock::image::*;
use boondock::Docker;

use std::default::Default;

#[macro_use]
mod common;

fn create_image_test<C>(docker: Docker<C>)
where
    C: Connect + Sync + 'static,
{
    rt_stream!(
        docker.create_image(Some(CreateImageOptions {
            from_image: String::from("hello-world"),
            ..Default::default()
        })),
        |result: Vec<CreateImageResults>| match result[0] {
            CreateImageResults::CreateImageProgressResponse {
                id: Some(ref id), ..
            } => assert_eq!(id, "latest"),
            _ => panic!(),
        }
    );
}

fn search_images_test<C>(docker: Docker<C>)
where
    C: Connect + Sync + 'static,
{
    rt_exec!(
        docker.search_images(SearchImagesOptions {
            term: "hello-world".to_string(),
            ..Default::default()
        }),
        |result: Vec<APIImageSearch>| assert!(
            result
                .into_iter()
                .any(|api_image| &api_image.name == "hello-world")
        )
    );
}

fn inspect_image_test<C>(docker: Docker<C>)
where
    C: Connect + Sync + 'static,
{
    rt_exec!(
        docker.inspect_image("hello-world"),
        |result: Image| assert_eq!(result.repo_tags[0], "hello-world:latest")
    );
}

fn list_images_test<C>(docker: Docker<C>)
where
    C: Connect + Sync + 'static,
{
    rt_exec!(
        docker.list_images(Some(ListImagesOptions {
            all: true,
            ..Default::default()
        })),
        |result: Vec<APIImages>| assert!(result.into_iter().any(|api_image| {
            api_image.repo_tags.unwrap_or(vec![String::new()])[0] == "hello-world:latest"
        }))
    );
}

fn image_history_test<C>(docker: Docker<C>)
where
    C: Connect + Sync + 'static,
{
    rt_exec!(
        docker.image_history("hello-world"),
        |result: Vec<ImageHistory>| assert!(
            result
                .into_iter()
                .take(1)
                .any(|history| history.tags.unwrap_or(vec![String::new()])[0]
                    == "hello-world:latest")
        )
    );
}

fn prune_images_test<C>(docker: Docker<C>)
where
    C: Connect + Sync + 'static,
{
    rt_exec!(
        docker.prune_images(None),
        |result: PruneImagesResults| assert_eq!(result.space_reclaimed, 0)
    );
}

fn remove_image_test<C>(docker: Docker<C>)
where
    C: Connect + Sync + 'static,
{
    rt_exec!(
        docker.remove_image(
            "hello-world",
            Some(RemoveImageOptions {
                noprune: true,
                ..Default::default()
            })
        ),
        |result: Vec<RemoveImageResults>| match result[0].to_owned() {
            RemoveImageResults::RemoveImageUntagged { untagged } => {
                assert_eq!(untagged, "hello-world:latest")
            }
            _ => panic!(),
        }
    );
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
    connect_to_docker_and_run!(create_image_test);
    connect_to_docker_and_run!(inspect_image_test);
}

#[test]
fn integration_test_image_create() {
    connect_to_docker_and_run!(create_image_test);
    connect_to_docker_and_run!(list_images_test);
}

#[test]
fn integration_test_image_history() {
    connect_to_docker_and_run!(create_image_test);
    connect_to_docker_and_run!(image_history_test);
}

#[test]
fn integration_test_prune_images() {
    connect_to_docker_and_run!(prune_images_test);
}

#[test]
fn integration_test_remove_image() {
    connect_to_docker_and_run!(create_image_test);
    connect_to_docker_and_run!(remove_image_test);
}
