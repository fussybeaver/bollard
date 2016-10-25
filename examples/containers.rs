extern crate boondock;

use boondock::Docker;

fn main() {
    let docker = Docker::connect_with_defaults().unwrap();
    let containers = docker.get_containers(false).unwrap();
    for container in &containers {
        println!("{}", container.Id);
    }
}
