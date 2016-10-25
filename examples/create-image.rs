extern crate boondock;

use boondock::Docker;

fn main() {
    let docker = Docker::connect_with_defaults().unwrap();

    let image = "debian".to_string();
    let tag = "latest".to_string();
    let statuses = docker.create_image(image, tag).unwrap();

    if let Some(last) = statuses.last() {
        println!("{}", last.clone().status.unwrap());
    } else {
        println!("none");
    }
}
