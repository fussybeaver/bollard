//! Builds a container with a bunch of extra options for testing
#![allow(unused_variables, unused_mut)]

#[tokio::main]
async fn main() {
    #[cfg(feature = "buildkit")]
    {
        use bollard::grpc::driver::docker_container::DockerContainerBuilder;
        use bollard::Docker;
        use std::io::Write;

        let mut docker = Docker::connect_with_socket_defaults().unwrap();

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

        let session_id = "bollard-oci-export-buildkit-example";

        let frontend_opts = bollard::grpc::export::ImageBuildFrontendOptions::builder()
            .pull(true)
            .build();

        let output = bollard::grpc::export::ImageExporterOCIOutputBuilder::new(
            "docker.io/library/bollard-oci-export-buildkit-example:latest",
        )
        .annotation("exporter", "Bollard")
        .dest(&std::path::Path::new("/tmp/oci-image.tar"));

        let buildkit_builder =
            DockerContainerBuilder::new("bollard_buildkit_export_oci_image", &docker, session_id);
        let driver = buildkit_builder.bootstrap().await.unwrap();

        let load_input =
            bollard::grpc::export::ImageExporterLoadInput::Upload(bytes::Bytes::from(compressed));

        docker
            .image_export_oci(driver, session_id, frontend_opts, output, load_input)
            .await
            .unwrap();
    }
}
