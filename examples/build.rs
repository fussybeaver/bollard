//! Builds a container with a bunch of extra options for testing

use bollard::image::BuildImageOptions;
use bollard::Docker;

use std::collections::HashMap;

use futures_util::stream::StreamExt;

#[tokio::main]
async fn main() {
    #[cfg(unix)]
    let docker = Docker::connect_with_unix_defaults().unwrap();
    #[cfg(windows)]
    let docker = Docker::connect_with_named_pipe_defaults().unwrap();

    let mut build_image_args = HashMap::new();
    build_image_args.insert("dummy", "value");

    let mut build_image_labels = HashMap::new();
    build_image_labels.insert("maintainer", "somemaintainer");

    let build_image_options = BuildImageOptions {
        dockerfile: "Dockerfile",
        t: "bollard-build-example",
        extrahosts: Some("myhost:127.0.0.1"),
        remote:
            "https://raw.githubusercontent.com/docker-library/openjdk/master/11/jdk/slim/Dockerfile",
        q: false,
        nocache: false,
        cachefrom: vec![],
        pull: true,
        rm: true,
        forcerm: true,
        memory: Some(120000000),
        memswap: Some(500000),
        cpushares: Some(2),
        cpusetcpus: "0-3",
        cpuperiod: Some(2000),
        cpuquota: Some(1000),
        buildargs: build_image_args,
        shmsize: Some(1000000),
        squash: false,
        labels: build_image_labels,
        networkmode: "host",
        platform: "linux/x86_64",
    };

    let mut image_build_stream = docker
        .build_image(build_image_options, None, None);

    while let Some(msg) = image_build_stream.next().await {
        println!("Message: {:?}", msg);
    }
}
