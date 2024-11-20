#![type_length_limit = "2097152"]

use bollard::errors::Error;
use bollard::Docker;
use bollard::swarm::*;

use tokio::runtime::Runtime;

#[macro_use]
pub mod common;
use crate::common::*;

async fn swarm_test(docker: Docker) -> Result<(), Error> {
    // init swarm
    let config = InitSwarmOptions {
        listen_addr: "0.0.0.0:2377",
        advertise_addr: "127.0.0.1",
    };
    let _ = &docker
        .init_swarm(config)
        .await?;

    // inspect swarm
    let inspection_result = &docker
        .inspect_swarm()
        .await?;
    assert!(inspection_result.join_tokens.as_ref().unwrap().worker.as_ref().unwrap().len() > 0);

    // leave swarm
    let config = LeaveSwarmOptions {
        force: true,
    };
    let _ = &docker
        .leave_swarm(Some(config))
        .await?;
    Ok(())
}

#[test]
fn integration_test_swarm() {
    connect_to_docker_and_run!(swarm_test);
}