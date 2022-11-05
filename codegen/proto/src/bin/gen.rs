use tonic_build;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_client(false)
        .out_dir("src/generated")
        .compile_well_known_types(true)
        .compile(
            &[
                "resources/moby/buildkit/v1/control.proto",
                "resources/moby/buildkit/v1/types/worker.proto",
                "resources/grpc/health/v1/health.proto",
            ],
            &["resources"],
        )?;
    Ok(())
}
