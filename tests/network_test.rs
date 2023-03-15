extern crate bollard_next;
extern crate hyper;
extern crate tokio;

use bollard_next::container::*;
use bollard_next::errors::Error;
use bollard_next::models::*;
use bollard_next::network::*;
use bollard_next::Docker;

use tokio::runtime::Runtime;

use std::collections::HashMap;

#[macro_use]
pub mod common;
use crate::common::*;

async fn create_network_test(docker: Docker) -> Result<(), Error> {
    let ipam_config = IpamConfig {
        subnet: Some(String::from("10.10.10.10/24")),
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
        ipam: Ipam {
            config: Some(vec![ipam_config]),
            ..Default::default()
        },
        ..Default::default()
    };

    let result = &docker.create_network(create_network_options).await?;
    let result = &docker
        .inspect_network(
            result.id.as_ref().unwrap(),
            Some(InspectNetworkOptions::<&str> {
                verbose: true,
                ..Default::default()
            }),
        )
        .await?;

    assert!(result
        .ipam
        .as_ref()
        .unwrap()
        .config
        .as_ref()
        .unwrap()
        .iter()
        .take(1)
        .any(|i| &i.subnet.as_ref().unwrap()[..] == "10.10.10.10/24"));

    let _ = &docker
        .remove_network("integration_test_create_network")
        .await?;

    Ok(())
}

async fn list_networks_test(docker: Docker) -> Result<(), Error> {
    let ipam_config = IpamConfig {
        subnet: Some(String::from("10.10.10.10/24")),
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
        ipam: Ipam {
            config: Some(vec![ipam_config]),
            ..Default::default()
        },
        labels: create_network_filters,
        ..Default::default()
    };

    let mut list_networks_filters = HashMap::new();
    list_networks_filters.insert("label", vec!["maintainer=bollard-maintainer"]);

    let _ = &docker.create_network(create_network_options).await?;

    let results = &docker
        .list_networks(Some(ListNetworksOptions {
            filters: list_networks_filters,
        }))
        .await?;

    let v = results.get(0).unwrap();

    assert!(v
        .ipam
        .as_ref()
        .unwrap()
        .config
        .as_ref()
        .unwrap()
        .iter()
        .any(|i| &i.subnet.as_ref().unwrap()[..] == "10.10.10.10/24"));

    let _ = &docker
        .remove_network("integration_test_list_network")
        .await?;

    Ok(())
}

async fn connect_network_test(docker: Docker) -> Result<(), Error> {
    let ipam_config = IpamConfig {
        subnet: Some(String::from("10.10.10.10/24")),
        ..Default::default()
    };

    let create_network_options = CreateNetworkOptions {
        name: "integration_test_connect_network",
        check_duplicate: true,
        ipam: Ipam {
            config: Some(vec![ipam_config]),
            ..Default::default()
        },
        ..Default::default()
    };

    let connect_network_options = ConnectNetworkOptions {
        container: "integration_test_connect_network_test",
        endpoint_config: EndpointSettings {
            ipam_config: Some(EndpointIpamConfig {
                ipv4_address: Some(String::from("10.10.10.101")),
                ..Default::default()
            }),
            ..Default::default()
        },
    };

    create_daemon(&docker, "integration_test_connect_network_test").await?;

    let result = &docker.create_network(create_network_options).await?;

    let _ = &docker
        .connect_network(result.id.as_ref().unwrap(), connect_network_options)
        .await?;

    let id = result.id.as_ref().unwrap();

    let result = &docker
        .inspect_network(
            id,
            Some(InspectNetworkOptions::<&str> {
                verbose: true,
                ..Default::default()
            }),
        )
        .await?;

    assert!(result
        .containers
        .as_ref()
        .unwrap()
        .iter()
        .any(|(_, container)| container.ipv4_address == Some("10.10.10.101/24".into())));

    let _ = &docker
        .disconnect_network(
            id,
            DisconnectNetworkOptions {
                container: "integration_test_connect_network_test",
                force: true,
            },
        )
        .await?;

    let _ = &docker.remove_network(id).await?;

    let _ = &docker
        .kill_container(
            "integration_test_connect_network_test",
            None::<KillContainerOptions<String>>,
        )
        .await?;

    let _ = &docker
        .remove_container(
            "integration_test_connect_network_test",
            None::<RemoveContainerOptions>,
        )
        .await?;

    Ok(())
}

async fn prune_networks_test(docker: Docker) -> Result<(), Error> {
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

    let _ = &docker.create_network(create_network_options).await?;

    let result = &docker
        .prune_networks(None::<PruneNetworksOptions<&str>>)
        .await?;

    assert_eq!(
        "integration_test_prune_networks",
        result.networks_deleted.as_ref().unwrap()[0]
    );

    let results = &docker
        .list_networks(Some(ListNetworksOptions {
            filters: list_networks_filters,
        }))
        .await?;

    assert_eq!(0, results.len());

    Ok(())
}

#[test]
#[cfg(unix)]
// Hangs on Appveyor
fn integration_test_create_network() {
    connect_to_docker_and_run!(create_network_test);
}

#[test]
#[cfg(unix)]
// Hangs on Appveyor
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
#[cfg(unix)]
// Hangs on Appveyor
fn integration_test_prune_networks() {
    connect_to_docker_and_run!(prune_networks_test);
}
