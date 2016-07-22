extern crate docker;

use docker::Docker;

fn main() {
    let mut docker = match Docker::connect("unix:///var/run/docker.sock") {
        Ok(docker) => docker,
        Err(e) => { panic!("{}", e); }
    };

    let containers = match docker.get_containers(false) {
        Ok(containers) => containers,
        Err(e) => { panic!("{}", e); }
    };
    for container in &containers {
        println!("{}", container.Id);
    }
}
