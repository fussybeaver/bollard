//! Removes old docker containers, images, volumes and networks

use bollard::{container::PruneContainersOptions, image::PruneImagesOptions, network::PruneNetworksOptions, volume::PruneVolumesOptions};
use bollard::Docker;
use chrono::{Duration, Utc};

use std::collections::HashMap;

const THRESHOLD_DAYS: i64 = 90;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    #[cfg(unix)]
    let docker = Docker::connect_with_unix_defaults()?;
    #[cfg(windows)]
    let docker = Docker::connect_with_named_pipe_defaults()?;

    let date = Utc::now() - Duration::days(THRESHOLD_DAYS);
    let timestamp = &date.timestamp().to_string()[..];

    let mut prune_filters = HashMap::new();
    prune_filters.insert("until", vec![timestamp]);

    let prune = docker.prune_containers(Some(PruneContainersOptions {
        filters: prune_filters.clone()
    })).await?;

    println!("{:?}", prune);

    let prune = docker.prune_images(Some(PruneImagesOptions {
        filters: prune_filters.clone()
    })).await?;

    println!("{:?}", prune);

    let prune = docker.prune_volumes(None::<PruneVolumesOptions<String>>).await?;

    println!("{:?}", prune);

    let prune = docker.prune_networks(Some(PruneNetworksOptions {
        filters: prune_filters.clone()
    })).await?;

    println!("{:?}", prune);

    Ok(())
}
