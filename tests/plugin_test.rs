#[macro_use]
pub mod common;

use bollard::errors::Error;
use bollard::query_parameters::{GetPluginPrivilegesOptionsBuilder, ListPluginsOptionsBuilder};
use bollard::Docker;
use std::collections::HashMap;

async fn list_plugins_test(docker: Docker) -> Result<(), Error> {
    let _plugins = docker
        .list_plugins(None::<bollard::query_parameters::ListPluginsOptions>)
        .await?;
    // Just verify the API works and returns without error
    Ok(())
}

async fn list_plugins_with_filter_test(docker: Docker) -> Result<(), Error> {
    let mut filters: HashMap<String, Vec<String>> = HashMap::new();
    filters.insert("capability".to_string(), vec!["volumedriver".to_string()]);

    let options = ListPluginsOptionsBuilder::default()
        .filters(&filters)
        .build();

    let _plugins = docker.list_plugins(Some(options)).await?;
    // Just verify the API works with filters
    Ok(())
}

async fn get_plugin_privileges_test(docker: Docker) -> Result<(), Error> {
    // Test with a well-known plugin from Docker Hub
    let options = GetPluginPrivilegesOptionsBuilder::default()
        .remote("vieux/sshfs:latest")
        .build();

    // This may fail if the plugin doesn't exist on Docker Hub or network issues
    // So we accept both success and specific errors
    match docker.get_plugin_privileges(options).await {
        Ok(_privileges) => {
            // API returned valid data
        }
        Err(Error::DockerResponseServerError { status_code, .. }) => {
            // 500 can happen if Docker Hub is unreachable or plugin not found
            // 404 if plugin doesn't exist
            assert!(status_code == 500 || status_code == 404);
        }
        Err(e) => return Err(e),
    }
    Ok(())
}

#[test]
#[cfg(not(windows))]
fn integration_test_list_plugins() {
    use crate::common::run_runtime;
    use tokio::runtime::Runtime;
    connect_to_docker_and_run!(list_plugins_test);
}

#[test]
#[cfg(not(windows))]
fn integration_test_list_plugins_with_filter() {
    use crate::common::run_runtime;
    use tokio::runtime::Runtime;
    connect_to_docker_and_run!(list_plugins_with_filter_test);
}

#[test]
fn integration_test_get_plugin_privileges() {
    use crate::common::run_runtime;
    use tokio::runtime::Runtime;
    connect_to_docker_and_run!(get_plugin_privileges_test);
}
