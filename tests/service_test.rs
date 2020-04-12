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

async fn service_list_test(docker: Docker) -> Result<(), Error> {
    let image = if cfg!(windows) {
        format!("{}nanoserver/iis", registry_http_addr())
    } else {
        format!("{}fussybeaver/uhttpd", registry_http_addr())
    };
    let spec = ServiceSpec {
        name: "integration_test_list_services",
        task_template: TaskSpec {
            container_spec: Some(TaskSpecContainerSpec {
                image: Some(&image),
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };

    docker.create_service(spec, None).await?;

    let mut response = docker
        .list_services(None::<ListServicesOptions<String>>)
        .await?;

    assert_eq!(response.len(), 1);
    assert_eq!(
        response.pop().unwrap().spec.name.as_str(),
        "integration_test_list_services"
    );

    docker
        .delete_service("integration_test_list_services")
        .await?;

    Ok(())
}

async fn service_update_test(docker: Docker) -> Result<(), Error> {
    env_logger::init();
    let image = if cfg!(windows) {
        format!("{}nanoserver/iis", registry_http_addr())
    } else {
        format!("{}fussybeaver/uhttpd", registry_http_addr())
    };
    let spec = ServiceSpec {
        name: "integration_test_update_service",
        task_template: TaskSpec {
            container_spec: Some(TaskSpecContainerSpec {
                image: Some(&image),
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };

    docker.create_service(spec, None).await?;

    let service_name = "integration_test_update_service";
    let current_version = docker
        .inspect_service(service_name, None::<InspectServiceOptions>)
        .await?
        .version;
    let service = ServiceSpec::<&str> {
        name: "integration_test_update_service",
        task_template: TaskSpec {
            container_spec: Some(TaskSpecContainerSpec {
                image: Some(&image),
                ..Default::default()
            }),
            ..Default::default()
        },
        mode: Some(ServiceSpecMode::Replicated { replicas: 0 }),
        ..Default::default()
    };
    let options = UpdateServiceOptions {
        version: current_version,
        ..Default::default()
    };
    let credentials = None;

    docker
        .update_service(service_name, service, options, credentials)
        .await?;

    let mut response = docker
        .list_services(None::<ListServicesOptions<String>>)
        .await?;

    assert_eq!(
        response.pop().unwrap().spec.mode.unwrap(),
        ServiceSpecMode::Replicated { replicas: 0 }
    );

    docker
        .delete_service("integration_test_update_service")
        .await?;

    Ok(())
}

#[test]
fn integration_test_create_service() {
    connect_to_docker_and_run!(service_create_test);
}

#[test]
fn integration_test_list_services() {
    connect_to_docker_and_run!(service_list_test);
}

#[test]
fn integration_test_update_service() {
    connect_to_docker_and_run!(service_update_test);
}
