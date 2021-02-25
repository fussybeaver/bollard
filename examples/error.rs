/// Example of using `std::error::Error` with bollard
extern crate bollard;

use bollard::Docker;

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let _docker1 = Docker::connect_with_unix_defaults()?;

    let _env_var = std::env::var("ZOOKEEPER_ADDR")?;

    Ok(())
}

fn main() {
    run().unwrap();

    ()
}
