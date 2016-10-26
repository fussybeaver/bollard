extern crate boondock;

use boondock::Docker;

fn main() {
    let docker = Docker::connect_with_defaults().unwrap();
    println!("{:#?}", docker.version().unwrap());
}
