//! Builds a container with a bunch of extra options for testing
#![allow(unused_variables, unused_mut)]

use bollard::image::{BuildImageOptions, BuilderVersion};
#[cfg(feature = "buildkit")]
use bollard::models::BuildInfoAux;
use bollard::Docker;

#[cfg(feature = "buildkit")]
use futures_util::stream::StreamExt;

use std::{collections::HashMap, io::Write};

#[tokio::main]
async fn main() {
    env_logger::init();
    let mut docker = Docker::connect_with_http_defaults().unwrap();

    let dockerfile = String::from(
        "FROM alpine as builder1
RUN touch bollard.txt
FROM alpine as builder2
RUN --mount=type=bind,from=builder1,target=mnt cp mnt/bollard.txt buildkit-bollard.txt
ENTRYPOINT ls buildkit-bollard.txt
",
    );

    let mut header = tar::Header::new_gnu();
    header.set_path("Dockerfile").unwrap();
    header.set_size(dockerfile.len() as u64);
    header.set_mode(0o755);
    header.set_cksum();
    let mut tar = tar::Builder::new(Vec::new());
    tar.append(&header, dockerfile.as_bytes()).unwrap();

    let uncompressed = tar.into_inner().unwrap();
    let mut c = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    c.write_all(&uncompressed).unwrap();
    let compressed = c.finish().unwrap();

    let mut attrs = HashMap::new();
    attrs.insert("name", "alpine-bollard");
    attrs.insert("dest", "/tmp/alpine-oci-image");
    attrs.insert("annotation.exporter", "Bollard");

    #[cfg(feature = "buildkit")]
    let outputs = vec![
        bollard::image::ImageBuildOutput { typ: "oci", attrs: attrs.clone() },
    ];

    let id = "bollard-oci-export-buildkit-example";
    let build_image_options = BuildImageOptions {
        t: id,
        version: BuilderVersion::BuilderBuildKit,
        pull: true,
        #[cfg(feature = "buildkit")]
        session: Some(String::from(id)),
        #[cfg(feature = "buildkit")]
        outputs: Some(outputs),
        ..Default::default()
    };

    docker.export_oci_image(build_image_options, Some(compressed)).await;
    //let mut image_build_stream =
    //    docker.build_image(build_image_options, None, Some(compressed.into()));

    //#[cfg(feature = "buildkit")]
    //while let Some(Ok(bollard::models::BuildInfo {
    //    aux: Some(BuildInfoAux::BuildKit(inner)),
    //    ..
    //})) = image_build_stream.next().await
    //{
    //    println!("Response: {:?}", inner);
    //}
}
