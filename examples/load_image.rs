// Load a Docker image tarball into the Docker daemon

use bollard::Docker;
use bollard::image::ImportImageOptions;
use futures_util::stream::StreamExt;
use tokio::fs::File;
use tokio_util::codec;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let docker = Docker::connect_with_local_defaults()?;
    
    // Path to your Docker image tarball (e.g., created with `docker save`)
    let tarball_path = std::env::args()
        .nth(1)
        .expect("Usage: load_image <path-to-image.tar>");
    
    // Open the tarball file
    let file = File::open(&tarball_path).await?;
    println!("Loading image from: {}", tarball_path);
    
    // Create a stream from the file
    let byte_stream = codec::FramedRead::new(file, codec::BytesCodec::new())
        .map(|r| r.unwrap().freeze());
    
    // Load the image
    let mut stream = docker.load_image_stream(
        ImportImageOptions::default(),
        byte_stream,
        None,
    );
    
    // Process the responses
    while let Some(result) = stream.next().await {
        match result {
            Ok(info) => {
                if let Some(status) = info.status {
                    println!("{}", status);
                }
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                return Err(e.into());
            }
        }
    }
    
    println!("Image loaded successfully!");
    Ok(())
}