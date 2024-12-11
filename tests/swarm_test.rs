#[macro_use]
pub mod common;

#[cfg(feature = "test_swarm")]
async fn swarm_test(docker: bollard::Docker) -> Result<(), bollard::errors::Error> {
    use bollard::swarm::*;

    // init swarm
    let config = InitSwarmOptions {
        listen_addr: "0.0.0.0:2377",
        advertise_addr: "127.0.0.1",
    };
    let _ = &docker.init_swarm(config).await?;

    // inspect swarm
    let inspection_result = &docker.inspect_swarm().await?;
    assert!(
        inspection_result
            .join_tokens
            .as_ref()
            .unwrap()
            .worker
            .as_ref()
            .unwrap()
            .len()
            > 0
    );

    // leave swarm
    let config = LeaveSwarmOptions { force: true };
    let _ = &docker.leave_swarm(Some(config)).await?;
    Ok(())
}

#[cfg(feature = "test_swarm")]
#[test]
fn integration_test_swarm() {
    use crate::common::run_runtime;
    use bollard::Docker;
    use tokio::runtime::Runtime;
    connect_to_docker_and_run!(swarm_test);
}
