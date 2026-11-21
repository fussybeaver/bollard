//! Builds a container with a bunch of extra options for testing
#![allow(unused_variables, unused_mut)]

mod common;

#[cfg(feature = "buildkit_providerless")]
use bollard::models::BuildInfoAux;
use bollard::Docker;

#[cfg(feature = "buildkit_providerless")]
use futures_util::stream::StreamExt;
use http_body_util::Full;

#[tokio::main]
async fn main() {
    let docker = Docker::connect_with_socket_defaults().unwrap();

    let dockerfile = String::from(
        "FROM alpine as builder1
RUN touch bollard.txt
FROM alpine as builder2
RUN --mount=type=bind,from=builder1,target=mnt cp mnt/bollard.txt buildkit-bollard.txt
ENTRYPOINT ls buildkit-bollard.txt
",
    );

    let compressed = common::gzip_tar([common::TarFile::new(
        "Dockerfile",
        dockerfile.into_bytes(),
        0o755,
    )])
    .unwrap();

    let id = "bollard-build-buildkit-example";
    let build_image_options = bollard::query_parameters::BuildImageOptionsBuilder::default()
        .t(id)
        .dockerfile("Dockerfile")
        .version(bollard::query_parameters::BuilderVersion::BuilderBuildKit)
        .pull("true");

    #[cfg(feature = "buildkit_providerless")]
    let build_image_options = build_image_options.session(id);

    let mut image_build_stream = docker.build_image(
        build_image_options.build(),
        None,
        Some(http_body_util::Either::Left(Full::new(compressed.into()))),
    );

    #[cfg(feature = "buildkit_providerless")]
    while let Some(Ok(bollard::models::BuildInfo {
        aux: Some(BuildInfoAux::BuildKit(inner)),
        ..
    })) = image_build_stream.next().await
    {
        println!("Response: {:?}", inner);
    }
}
