extern crate bollard;
extern crate hyper;
extern crate tokio;

use bollard::network::*;
use bollard::Docker;

use hyper::client::connect::Connect;
use hyper::rt::Future;
use tokio::runtime::Runtime;

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

#[test]
#[cfg(unix)]
// Appveyor Windows error: "HNS failed with error : Unspecified error"
fn integration_test_create_network() {
    connect_to_docker_and_run!(create_network_test);
}
