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

    // test update swarm - get current version and spec
    let swarm = docker.inspect_swarm().await?;
    let version = swarm.version.unwrap().index.unwrap();
    let spec = swarm.spec.unwrap();

    // update swarm (no changes, just verify API works)
    let options = UpdateSwarmOptions {
        version,
        ..Default::default()
    };
    docker.update_swarm(spec, options).await?;

    // verify swarm version incremented after update
    let updated_swarm = docker.inspect_swarm().await?;
    assert!(updated_swarm.version.unwrap().index.unwrap() > version);

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
