extern crate boondock;

use boondock::Docker;

fn main() {
    let docker = Docker::connect_with_defaults().unwrap();
    let images = docker.images(false).unwrap();

    for image in &images {
    	println!("{} -> Size: {}, Virtual Size: {}, Created: {}", image.Id, image.Size, image.VirtualSize, image.Created);
    }
}
