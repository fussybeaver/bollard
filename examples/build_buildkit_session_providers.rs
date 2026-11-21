//! Builds an image through the Docker daemon's classic `/build` endpoint with
//! BuildKit enabled, serving a build secret over the session via
//! `build_image_with_session_providers` — unlike the GRPC driver examples, the
//! response stream carries the full BuildKit build progress/log output.

mod common;

use bollard::grpc::build::{ImageBuildSessionProviders, SecretSource};
use bollard::models::BuildInfoAux;
use bollard::Docker;

use futures_util::stream::StreamExt;
#[tokio::main]
async fn main() {
    let docker = Docker::connect_with_socket_defaults().unwrap();

    let dockerfile = String::from(
        "FROM alpine
RUN --mount=type=secret,id=token echo \"the secret is: $(cat /run/secrets/token)\"
",
    );

    let compressed = common::gzip_tar([common::TarFile::new(
        "Dockerfile",
        dockerfile.into_bytes(),
        0o755,
    )])
    .unwrap();

    std::env::set_var("EXAMPLE_BUILD_SECRET", "buildkit-session-provider-example");

    let id = "bollard-build-buildkit-session-providers-example";
    let build_image_options = bollard::query_parameters::BuildImageOptionsBuilder::default()
        .t(id)
        .dockerfile("Dockerfile")
        .version(bollard::query_parameters::BuilderVersion::BuilderBuildKit)
        .nocache(true)
        .session(id)
        .build();

    let providers = ImageBuildSessionProviders::default().set_secret(
        "token",
        &SecretSource::Env(String::from("EXAMPLE_BUILD_SECRET")),
    );

    let mut image_build_stream = docker.build_image_with_session_providers(
        build_image_options,
        None,
        Some(http_body_util::Either::Left(http_body_util::Full::new(
            compressed.into(),
        ))),
        providers,
    );

    while let Some(msg) = image_build_stream.next().await {
        match msg {
            Ok(bollard::models::BuildInfo {
                aux: Some(BuildInfoAux::BuildKit(inner)),
                ..
            }) => {
                for log in &inner.logs {
                    print!("{}", String::from_utf8_lossy(&log.msg));
                }
            }
            Ok(other) => println!("Response: {other:?}"),
            Err(err) => panic!("build failed: {err}"),
        }
    }
}
