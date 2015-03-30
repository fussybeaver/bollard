extern crate docker;

use docker::Docker;

fn main() {
    let docker = Docker::new();
    let containers = match docker.get_containers() {
        Ok(containers) => containers,
        Err(e) => { panic!("{}", e); }
    };
    
    for container in containers.iter() {
        let stats = match docker.get_stats(&container) {
            Ok(stats) => stats,
            Err(e) => { panic!("{}", e); }
        };
    }
}
