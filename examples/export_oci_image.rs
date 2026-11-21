//! Exports an image using an ephemeral BuildKit builder.
#![allow(unused_variables, unused_mut)]

#[cfg(feature = "buildkit_providerless")]
mod common;

#[tokio::main]
async fn main() {
    #[cfg(feature = "buildkit_providerless")]
    {
        use bollard::grpc::driver::docker_container::{
            DockerContainerBuilder, DockerContainerLifecycle,
        };
        use bollard::Docker;
        let mut docker = Docker::connect_with_socket_defaults().unwrap();

        let dockerfile = String::from(
            "FROM localhost:5000/alpine as builder1
            RUN touch bollard.txt
            FROM localhost:5000/alpine as builder2
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

        let session_id = "bollard-oci-export-buildkit-example";

        let frontend_opts = bollard::grpc::build::ImageBuildFrontendOptions::builder()
            .pull(true)
            .build();

        let output = bollard::grpc::export::ImageExporterOutput::builder(
            "docker.io/library/bollard-oci-export-buildkit-example:latest",
        )
        .annotation("exporter", "Bollard")
        .dest(std::path::Path::new("/tmp/oci-image.tar"));

        let mut buildkit_builder = DockerContainerBuilder::new(&docker);
        buildkit_builder
            .name("bollard-oci-export-buildkit-example")
            .lifecycle(DockerContainerLifecycle::RemoveAfterSolve { keep_state: false });
        buildkit_builder.env("JAEGER_TRACE=localhost:6831");
        let driver = buildkit_builder.bootstrap().await.unwrap();

        let load_input =
            bollard::grpc::build::ImageBuildLoadInput::Upload(bytes::Bytes::from(compressed));

        let credentials = bollard::auth::DockerCredentials {
            username: Some("bollard".to_string()),
            password: std::env::var("REGISTRY_PASSWORD").ok(),
            ..Default::default()
        };
        let mut creds_hsh = std::collections::HashMap::new();
        creds_hsh.insert("localhost:5000", credentials);

        bollard::grpc::driver::Export::export(
            &driver,
            bollard::grpc::driver::ImageExporterEnum::OCI(output),
            frontend_opts,
            load_input,
            Some(creds_hsh),
            None,
        )
        .await
        .unwrap();
    }
}
