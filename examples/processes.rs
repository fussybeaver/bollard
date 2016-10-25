extern crate boondock;

use boondock::Docker;

fn main() {
    let docker = Docker::connect_with_defaults().unwrap();
    if let Some(container) = docker.get_containers(false).unwrap().get(0) {
        for process in docker.get_processes(container).unwrap() {
            println!("{:#?}", process);
        }
    }
}
