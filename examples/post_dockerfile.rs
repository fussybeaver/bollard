//! Post a dockerfile 
//!
//! tar cvf dockerfile.tar Dockerfile

use bollard::image::BuildImageOptions;
use bollard::Docker;
use futures_util::stream::StreamExt;
use hyper::body::Body;

use tokio::fs::File;
use tokio_util::codec::{BytesCodec, FramedRead};

use std::env::args;

#[tokio::main]
async fn main() {
    let docker = Docker::connect_with_unix_defaults().unwrap();

    let image_options = BuildImageOptions {
        dockerfile: "Dockerfile",
        t: "rust-test",
        rm: true,
        ..Default::default()
    };
    
    let filename = &args().nth(1).expect("needs first argument");
    let archive = File::open(filename).await.expect("could not open file");
    let stream = FramedRead::new(archive, BytesCodec::new());
    let body = Body::wrap_stream(stream);

    let mut image_build_stream = docker.build_image(image_options, None, Some(body));

    while let Some(msg) = image_build_stream.next().await {
        println!("Message: {:?}", msg);
    }
}
