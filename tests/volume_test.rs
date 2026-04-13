extern crate bollard;
extern crate hyper;
extern crate tokio;

use bollard::errors::Error;
use bollard::models::VolumeCreateRequest;
#[cfg(feature = "test_csi")]
use bollard::models::{
    ClusterVolumeSpec, ClusterVolumeSpecAccessMode, ClusterVolumeSpecAccessModeAvailabilityEnum,
    ClusterVolumeSpecAccessModeScopeEnum, ClusterVolumeSpecAccessModeSharingEnum,
};
use bollard::query_parameters::{
    ListVolumesOptionsBuilder, PruneVolumesOptionsBuilder, RemoveVolumeOptionsBuilder,
};
use bollard::Docker;

use tokio::runtime::Runtime;

use std::collections::HashMap;

#[macro_use]
pub mod common;
use crate::common::*;

async fn list_volumes_test(docker: Docker) -> Result<(), Error> {
    let mut create_volume_filters = HashMap::new();
    create_volume_filters.insert(
        String::from("maintainer"),
        String::from("bollard-maintainer"),
    );

    let create_volume_options = VolumeCreateRequest {
        name: Some(String::from("integration_test_list_volumes")),
        labels: Some(create_volume_filters),
        ..Default::default()
    };

    let mut list_volumes_filters = HashMap::new();
    list_volumes_filters.insert("label", vec!["maintainer=bollard-maintainer"]);

    let _ = &docker.create_volume(create_volume_options).await?;

    let list_volumes_options = ListVolumesOptionsBuilder::default()
        .filters(&list_volumes_filters)
        .build();

    let results = &docker.list_volumes(Some(list_volumes_options)).await?;

    assert_eq!(results.volumes.as_ref().unwrap().len(), 1);
    assert_eq!(
        results.volumes.as_ref().unwrap()[0].name,
        "integration_test_list_volumes"
    );

    let remove_volume_options = RemoveVolumeOptionsBuilder::default().force(true).build();
    let _ = &docker
        .remove_volume("integration_test_list_volumes", Some(remove_volume_options))
        .await?;

    Ok(())
}

async fn create_volume_test(docker: Docker) -> Result<(), Error> {
    let create_volume_options = VolumeCreateRequest {
        name: Some(String::from("integration_test_create_volume")),
        ..Default::default()
    };

    let create_result = &docker.create_volume(create_volume_options).await?;
    let inspect_result = &docker.inspect_volume(&create_result.name).await?;

    assert_eq!(inspect_result.name, "integration_test_create_volume");

    let remove_volume_options = RemoveVolumeOptionsBuilder::default().force(true).build();
    let _ = &docker
        .remove_volume(
            "integration_test_create_volume",
            Some(remove_volume_options),
        )
        .await?;

    Ok(())
}

async fn prune_volumes_test(docker: Docker) -> Result<(), Error> {
    let mut create_volume_filters = HashMap::new();
    create_volume_filters.insert(
        String::from("maintainer"),
        String::from("shiplift-maintainer"),
    );

    let create_volume_options = VolumeCreateRequest {
        name: Some(String::from("integration_test_prune_volumes_1")),
        labels: Some(create_volume_filters),
        ..Default::default()
    };

    let _ = &docker.create_volume(create_volume_options).await?;

    // --

    let mut create_volume_filters = HashMap::new();
    create_volume_filters.insert(
        String::from("maintainer"),
        String::from("bollard-maintainer"),
    );

    let create_volume_options = VolumeCreateRequest {
        name: Some(String::from("integration_test_prune_volumes_2")),
        labels: Some(create_volume_filters),
        ..Default::default()
    };

    let _ = &docker.create_volume(create_volume_options).await?;

    // --

    let mut prune_volumes_filters = HashMap::new();
    if cfg!(not(windows)) {
        prune_volumes_filters.insert("all", vec!["true"]);
    }
    prune_volumes_filters.insert("label!", vec!["maintainer=bollard-maintainer"]);

    let prune_volumes_options = PruneVolumesOptionsBuilder::default()
        .filters(&prune_volumes_filters)
        .build();

    let _ = &docker.prune_volumes(Some(prune_volumes_options)).await?;

    // Varying Result objects depending on platform / Docker server version
    // - the volumes are still pruned though
    //if cfg!(not(feature = "test_macos")) {
    //    assert!(result
    //        .volumes_deleted
    //        .as_ref()
    //        .unwrap()
    //        .iter()
    //        .any(|v| v == "integration_test_prune_volumes_1"));
    //}

    let mut list_volumes_filters = HashMap::new();
    list_volumes_filters.insert("label", vec!["maintainer=shiplift-maintainer"]);

    let list_volumes_options = ListVolumesOptionsBuilder::default()
        .filters(&list_volumes_filters)
        .build();

    let results = &docker.list_volumes(Some(list_volumes_options)).await?;

    if cfg!(windows) {
        assert_eq!(results.volumes, None);
    } else {
        assert_eq!(results.volumes, Some(vec![]));
    }

    let mut list_volumes_filters = HashMap::new();
    list_volumes_filters.insert("label", vec!["maintainer=bollard-maintainer"]);

    let list_volumes_options = ListVolumesOptionsBuilder::default()
        .filters(&list_volumes_filters)
        .build();

    let results = &docker.list_volumes(Some(list_volumes_options)).await?;

    assert_eq!(results.volumes.as_ref().unwrap().len(), 1);
    assert_eq!(
        results.volumes.as_ref().unwrap()[0].name,
        "integration_test_prune_volumes_2"
    );

    let results = &docker
        .list_volumes(None::<bollard::query_parameters::ListVolumesOptions>)
        .await?;

    assert_ne!(0, results.volumes.as_ref().unwrap().len());

    let remove_volume_options = RemoveVolumeOptionsBuilder::default().force(true).build();
    let _ = &docker
        .remove_volume(
            "integration_test_prune_volumes_2",
            Some(remove_volume_options),
        )
        .await?;

    Ok(())
}

#[cfg(feature = "test_csi")]
async fn cluster_volume_accessible_topology_test(docker: Docker) -> Result<(), Error> {
    let create_options = VolumeCreateRequest {
        name: Some("integration_test_cluster_volume".into()),
        driver: Some("csi-local-path".into()),
        cluster_volume_spec: Some(ClusterVolumeSpec {
            access_mode: Some(ClusterVolumeSpecAccessMode {
                scope: Some(ClusterVolumeSpecAccessModeScopeEnum::SINGLE),
                sharing: Some(ClusterVolumeSpecAccessModeSharingEnum::ALL),
                mount_volume: Some(Default::default()),
                availability: Some(ClusterVolumeSpecAccessModeAvailabilityEnum::ACTIVE),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    };

    let _ = docker.create_volume(create_options).await?;

    // Volume provisioning is asynchronous: poll until ClusterVolume.Info is populated.
    let info = {
        let mut retries = 30;
        loop {
            let volume = docker
                .inspect_volume("integration_test_cluster_volume")
                .await?;
            let cluster = volume.cluster_volume.expect("expected cluster volume");
            if let Some(info) = cluster.info {
                break info;
            }
            retries -= 1;
            assert!(retries > 0, "timed out waiting for cluster volume info");
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    };

    let volume_full = docker
        .inspect_volume("integration_test_cluster_volume")
        .await?;
    println!(
        "Full volume JSON: {}",
        serde_json::to_string_pretty(&volume_full).unwrap()
    );
    println!(
        "ClusterVolumeInfo JSON: {}",
        serde_json::to_string_pretty(&info).unwrap()
    );

    let topology = info
        .accessible_topology
        .expect("expected accessible_topology");
    assert!(
        !topology.is_empty(),
        "accessible_topology should be non-empty"
    );
    assert!(
        topology[0]
            .segments
            .as_ref()
            .map_or(false, |s| !s.is_empty()),
        "Topology.Segments should be non-empty"
    );

    let remove_volume_options = RemoveVolumeOptionsBuilder::default().force(true).build();
    let _ = docker
        .remove_volume(
            "integration_test_cluster_volume",
            Some(remove_volume_options),
        )
        .await?;

    Ok(())
}

#[test]
fn integration_test_list_volumes() {
    connect_to_docker_and_run!(list_volumes_test);
}

#[test]
fn integration_test_create_volume() {
    connect_to_docker_and_run!(create_volume_test);
}

#[test]
fn integration_test_prune_volumes() {
    connect_to_docker_and_run!(prune_volumes_test);
}
#[cfg(feature = "test_csi")]
#[test]
fn integration_test_cluster_volume_accessible_topology() {
    connect_to_docker_and_run!(cluster_volume_accessible_topology_test);
}
