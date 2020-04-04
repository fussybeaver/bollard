use bollard::errors::Error;
use bollard::{service::*, Docker};

use tokio::runtime::Runtime;

#[macro_use]
mod common;
use crate::common::*;

async fn service_create_test(docker: Docker) -> Result<(), Error> {
    let image = if cfg!(windows) {
        format!("{}nanoserver/iis", registry_http_addr())
    } else {
        format!("{}fussybeaver/uhttpd", registry_http_addr())
    };
    let spec = ServiceSpec {
        name: "integration_test_create_service",
        task_template: TaskSpec {
            container_spec: Some(TaskSpecContainerSpec {
                image: Some(&image),
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };

    let respone = docker.create_service(spec, None).await?;

    assert_ne!(respone.id.len(), 0);

    docker
        .delete_service("integration_test_create_service")
        .await?;

    Ok(())
}

#[test]
fn integration_test_create_service() {
    connect_to_docker_and_run!(service_create_test);
}
