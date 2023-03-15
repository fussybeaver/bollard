//! Removes old docker containers, images, volumes and networks

use bollard_next::Docker;
use bollard_next::{
    container::PruneContainersOptions, image::PruneImagesOptions, network::PruneNetworksOptions,
    volume::PruneVolumesOptions,
};
use std::collections::HashMap;

const THRESHOLD_DAYS: i64 = 90;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    let docker = Docker::connect_with_socket_defaults().unwrap();

    #[cfg(feature = "time")]
    let timestamp = {
        let date = time::OffsetDateTime::now_utc() - time::Duration::days(THRESHOLD_DAYS);
        &date.unix_timestamp().to_string()[..]
    };

    #[cfg(feature = "chrono")]
    let timestamp = {
        let date = chrono::Utc::now() - chrono::Duration::days(THRESHOLD_DAYS);
        &date.timestamp().to_string()[..]
    };

    #[cfg(not(any(feature = "time", feature = "chrono")))]
    let timestamp = {
        use std::convert::TryInto;
        let date = std::time::SystemTime::now()
            - std::time::Duration::from_secs((THRESHOLD_DAYS * 86400).try_into().unwrap());
        &date
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_string()[..]
    };

    let mut prune_filters = HashMap::new();
    prune_filters.insert("until", vec![timestamp]);

    let prune = docker
        .prune_containers(Some(PruneContainersOptions {
            filters: prune_filters.clone(),
        }))
        .await?;

    println!("{:?}", prune);

    let prune = docker
        .prune_images(Some(PruneImagesOptions {
            filters: prune_filters.clone(),
        }))
        .await?;

    println!("{:?}", prune);

    let prune = docker
        .prune_volumes(None::<PruneVolumesOptions<String>>)
        .await?;

    println!("{:?}", prune);

    let prune = docker
        .prune_networks(Some(PruneNetworksOptions {
            filters: prune_filters.clone(),
        }))
        .await?;

    println!("{:?}", prune);

    Ok(())
}
