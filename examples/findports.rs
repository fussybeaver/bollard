extern crate boondock;

use boondock::{ContainerListOptions, Docker};
use boondock::errors::*;
use std::io::{self, Write};

fn find_all_exported_ports() -> Result<()> {
    let docker = try!(Docker::connect_with_defaults());
    let containers = try!(docker.containers(ContainerListOptions::default()));
    for container in &containers {
        let info = try!(docker.container_info(&container));

        // Uncomment this to dump everything we know about a container.
        //println!("{:#?}", &info);

        if let Some(ports) = info.NetworkSettings.Ports {
            let ports: Vec<String> = ports.keys().cloned().collect();
            println!("{}: {}", &info.Name, ports.join(", "));
        }
    }
    Ok(())
}

fn main() {
    if let Err(err) = find_all_exported_ports() {
        write!(io::stderr(), "Error: ").unwrap();
        for e in err.iter() {
            write!(io::stderr(), "{}\n", e).unwrap();
        }
    }
}
