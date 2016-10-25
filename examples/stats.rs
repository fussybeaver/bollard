extern crate boondock;

use boondock::Docker;

fn main() {
    let docker = Docker::connect_with_defaults().unwrap();
    if let Some(container) = docker.get_containers(false).unwrap().get(0) {
        for stats in docker.get_stats(container).unwrap() {
            println!("{:#?}", stats);
        }
    }
}
