extern crate docker;

use docker::Docker;

fn main() {
    let mut docker = Docker::connect_with_defaults().unwrap();
    let containers = docker.get_containers(false).unwrap();
    for container in &containers {
        println!("{}", container.Id);
    }
}
