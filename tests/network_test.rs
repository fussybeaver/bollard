extern crate bollard;
extern crate hyper;
extern crate tokio;

use bollard::container::*;
use bollard::network::*;
use bollard::Docker;

use hyper::rt::Future;
use tokio::runtime::Runtime;

use std::collections::HashMap;

#[macro_use]
pub mod common;
use crate::common::*;

fn create_network_test(docker: Docker) {
    let rt = Runtime::new().unwrap();

    let ipam_config = IPAMConfig {
        subnet: Some("10.10.10.10/24"),
        ..Default::default()
    };

    let create_network_options = CreateNetworkOptions {
        name: "integration_test_create_network",
        check_duplicate: true,
        driver: if cfg!(windows) {
            "transparent"
        } else {
            "bridge"
        },
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
            docker.inspect_network(
                &id,
                Some(InspectNetworkOptions::<&str> {
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

fn list_networks_test(docker: Docker) {
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
        driver: if cfg!(windows) {
            "transparent"
        } else {
            "bridge"
        },
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

fn connect_network_test(docker: Docker) {
    let rt = Runtime::new().unwrap();

    let ipam_config = IPAMConfig {
        subnet: Some("10.10.10.10/24"),
        ..Default::default()
    };

    let create_network_options = CreateNetworkOptions {
        name: "integration_test_connect_network",
        check_duplicate: true,
        ipam: IPAM {
            config: vec![ipam_config],
            ..Default::default()
        },
        ..Default::default()
    };

    let connect_network_options = ConnectNetworkOptions {
        container: "integration_test_connect_network_test",
        endpoint_config: EndpointSettings {
            ipam_config: EndpointIPAMConfig {
                ipv4_address: "10.10.10.101",
                ..Default::default()
            },
            ..Default::default()
        },
    };

    let future = chain_create_daemon(docker.chain(), "integration_test_connect_network_test")
        .and_then(|docker| docker.create_network(create_network_options))
        .and_then(|(docker, result)| {
            docker
                .connect_network(&result.id, connect_network_options)
                .map(|(docker, _)| (docker, result.id))
        })
        .and_then(|(docker, id)| {
            docker
                .inspect_network(
                    &id,
                    Some(InspectNetworkOptions::<&str> {
                        verbose: true,
                        ..Default::default()
                    }),
                )
                .map(|(docker, result)| (docker, id, result))
        })
        .map(|(docker, id, result)| {
            assert!(result
                .containers
                .into_iter()
                .any(|(_, container)| container.ipv4_address == "10.10.10.101/24"));
            (docker, id)
        })
        .and_then(|(docker, id)| {
            docker
                .disconnect_network(
                    &id,
                    DisconnectNetworkOptions {
                        container: "integration_test_connect_network_test",
                        force: true,
                    },
                )
                .map(|(docker, _)| (docker, id))
        })
        .and_then(|(docker, id)| docker.remove_network(&id))
        .and_then(|(docker, _)| {
            docker.kill_container(
                "integration_test_connect_network_test",
                None::<KillContainerOptions<String>>,
            )
        })
        .and_then(|(docker, _)| {
            docker.remove_container(
                "integration_test_connect_network_test",
                None::<RemoveContainerOptions>,
            )
        });

    run_runtime(rt, future);
}

fn prune_networks_test(docker: Docker) {
    let rt = Runtime::new().unwrap();

    let create_network_options = CreateNetworkOptions {
        name: "integration_test_prune_networks",
        attachable: true,
        driver: if cfg!(windows) {
            "transparent"
        } else {
            "bridge"
        },
        check_duplicate: true,
        ..Default::default()
    };

    let mut list_networks_filters = HashMap::new();
    list_networks_filters.insert("scope", vec!["global"]);

    let future = docker
        .chain()
        .create_network(create_network_options)
        .and_then(|(docker, _)| docker.prune_networks(None::<PruneNetworksOptions<&str>>))
        .map(|(docker, result)| {
            assert_eq!(
                "integration_test_prune_networks",
                result.networks_deleted.unwrap()[0]
            );
            docker
        })
        .and_then(move |docker| {
            docker.list_networks(Some(ListNetworksOptions {
                filters: list_networks_filters,
            }))
        })
        .map(|(_, results)| assert_eq!(0, results.len()));

    run_runtime(rt, future);
}

#[test]
fn integration_test_create_network() {
    connect_to_docker_and_run!(create_network_test);
}

#[test]
fn integration_test_list_networks() {
    connect_to_docker_and_run!(list_networks_test);
}

#[test]
#[cfg(unix)]
// Not possible to test this on Appveyor...
fn integration_test_connect_network() {
    connect_to_docker_and_run!(connect_network_test);
}

#[test]
fn integration_test_prune_networks() {
    connect_to_docker_and_run!(prune_networks_test);
}
