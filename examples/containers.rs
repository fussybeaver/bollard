extern crate boondock;

use boondock::{ContainerListOptions, Docker};

fn main() {
    let docker = Docker::connect_with_defaults().unwrap();
    let opts = ContainerListOptions::default().all();
    let containers = docker.containers(opts).unwrap();
    for container in &containers {
        println!("{}", container.Id);
    }
}
