use tonic_build;


fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
         .build_server(false)
         .out_dir("src/proto")
         .compile(
             &[
                    "codegen/buildkit/session/secrets/secrets.proto",
                    "codegen/buildkit/session/sshforward/ssh.proto",
                 ],
             &["codegen/buildkit/session"],
         )?;
    // tonic_build::configure()
    //      .build_server(false)
    //      .out_dir("src/proto")
    //      .compile(
    //          &[
    //                 "codegen/buildkit/api/services/control/control.proto",
    //                 "codegen/buildkit/api/types/worker.proto",
    //              ],
    //          &["codegen/buildkit/api"],
    //      )?;
    Ok(())
 }
