extern crate docker;
extern crate rustc_serialize;

use docker::Docker;
use docker::container::Port;
use docker::stats::Stats;
use rustc_serialize::json;

#[derive(RustcEncodable, RustcDecodable)]
#[allow(non_snake_case)]
struct Data {
    Id: String,
    Image: String,
    Status: String,
    Command: String,
    Created: u64,
    Names: Vec<String>,
    Ports: Vec<Port>,
    Stats: Stats
}

fn main() {
    let docker = Docker::new();
    let containers = match docker.get_containers(true) {
        Ok(containers) => containers,
        Err(e) => { panic!("{}", e); }
    };
    
    for container in containers.iter() {
        match docker.get_stats(&container) {
            Ok(stats) => {
                let data = Data {
                    Id: container.Id.clone(),
                    Image: container.Image.clone(),
                    Status: container.Status.clone(),
                    Command: container.Command.clone(),
                    Created: container.Created.clone(),
                    Names: container.Names.clone(),
                    Ports: container.Ports.clone(),
                    Stats: stats
                };

                let encoded_container = json::encode(&data).unwrap();
                println!("{}", encoded_container);
            },
            Err(e) => { panic!("{}", e); }
        };
    }

    let info = match docker.get_info() {
        Ok(info) => info,
        Err(e) => { panic!("{}", e); }
    };
    
    println!("{}", info.Name);
}
