extern crate bollard;
extern crate hyper;
extern crate tokio;

use bollard::network::*;
use bollard::Docker;

use hyper::client::connect::Connect;
use hyper::rt::Future;
use tokio::runtime::Runtime;

use std::collections::HashMap;

#[macro_use]
pub mod common;
use common::*;

fn create_network_test<C>(docker: Docker<C>)
where
    C: Connect + Sync + 'static,
{
    let rt = Runtime::new().unwrap();

    let ipam_config = IPAMConfig {
        subnet: Some("10.10.10.10/24"),
        ..Default::default()
    };
    let create_network_options = CreateNetworkOptions {
        name: "integration_test_create_network",
        check_duplicate: true,
        ipam: IPAM {
            config: vec![ipam_config],
            ..Default::default()
        },
        ..Default::default()
    };

    let future = docker
        .chain()
        .create_network(create_network_options)
        .map(|(docker, result)| (docker, result.id))
        .and_then(move |(docker, id)| {
            docker.inspect_network::<_, _, String>(
                &id,
                Some(InspectNetworkOptions {
                    verbose: true,
                    ..Default::default()
                }),
            )
        })
        .map(|(docker, result)| {
            assert!(result
                .ipam
                .config
                .into_iter()
                .take(1)
                .any(|i| i.subnet.unwrap() == "10.10.10.10/24"));
            docker
        })
        .and_then(|docker| docker.remove_network("integration_test_create_network"));

    run_runtime(rt, future);
}

fn list_networks_test<C>(docker: Docker<C>)
where
    C: Connect + Sync + 'static,
{
    let rt = Runtime::new().unwrap();

    let ipam_config = IPAMConfig {
        subnet: Some("10.10.10.10/24"),
        ..Default::default()
    };

    let mut create_network_filters = HashMap::new();
    create_network_filters.insert("maintainer", "bollard-maintainer");

    let create_network_options = CreateNetworkOptions {
        name: "integration_test_list_network",
        check_duplicate: true,
        ipam: IPAM {
            config: vec![ipam_config],
            ..Default::default()
        },
        labels: create_network_filters,
        ..Default::default()
    };

    let mut list_networks_filters = HashMap::new();
    list_networks_filters.insert("label", vec!["maintainer=bollard-maintainer"]);

    let future = docker
        .chain()
        .create_network(create_network_options)
        .and_then(move |(docker, _)| {
            docker.list_networks(Some(ListNetworksOptions {
                filters: list_networks_filters,
            }))
        })
        .map(|(docker, results)| {
            assert!(results
                .into_iter()
                .take(1)
                .map(|v| v.ipam.config)
                .flatten()
                .any(|i| i.subnet.unwrap() == "10.10.10.10/24"));
            docker
        })
        .and_then(|docker| docker.remove_network("integration_test_list_network"));

    run_runtime(rt, future);
}

#[test]
#[cfg(unix)]
// Appveyor Windows error: "HNS failed with error : Unspecified error"
fn integration_test_create_network() {
    connect_to_docker_and_run!(create_network_test);
}

#[test]
#[cfg(unix)]
// Appveyor Windows error: "HNS failed with error : Unspecified error"
fn integration_test_list_networks() {
    connect_to_docker_and_run!(list_networks_test);
}
