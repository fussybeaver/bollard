//! Builds a container with a bunch of extra options for testing

use bollard::image::{BuildImageOptions, BuildImageResults};
use bollard::Docker;

use std::collections::HashMap;

use futures_util::stream::StreamExt;
use futures_util::try_stream::TryStreamExt;
use tokio::runtime::Runtime;

fn main() {
    let rt = Runtime::new().unwrap();
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

    let future = run(docker, build_image_options);

    rt.block_on(future).unwrap();
}

async fn run<'a>(
    docker: Docker,
    build_image_options: BuildImageOptions<&'a str>,
) -> Result<(), bollard::errors::Error> {
    docker
        .build_image(build_image_options, None, None)
        .map(|v| {
            println!("{:?}", v);
            v
        })
        .map_err(|e| {
            println!("{:?}", e);
            e
        })
        .collect::<Vec<Result<BuildImageResults, bollard::errors::Error>>>()
        .await;
    Ok(())
}
