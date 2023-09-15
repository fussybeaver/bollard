use tonic_build;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .out_dir("src/generated")
        .compile_well_known_types(true)
        .compile(
            &[
                "resources/fsutil/types/stat.proto",
                "resources/fsutil/types/wire.proto",
                "resources/moby/buildkit/v1/control.proto",
                "resources/moby/buildkit/v1/types/worker.proto",
                "resources/moby/filesync/v1/filesync.proto",
                "resources/moby/upload/v1/upload.proto",
                "resources/grpc/health/v1/health.proto",
            ],
            &["resources"],
        )?;
    Ok(())
}
