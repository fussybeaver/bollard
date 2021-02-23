use bollard::models::CreateImageInfo;
/// Example of creating an image from scratch with a file system in a raw Tar (or tar.gz) archive.
/// Run with `cargo run --example image_from_scratch <path to archive>.tar.gz
/// This implementation streams the archive file piece by piece to the Docker daemon,
/// but does so inefficiently. For best results, use `tokio::fs` instead of `std::fs`.
use bollard::{image::CreateImageOptions, Docker};
use futures_util::stream::{Stream, TryStreamExt};
use futures_util::task::{Context, Poll};
use hyper::body::Body;
use std::env::args;
use std::fs::File;
use std::io::{Read, Result as IOResult};
use std::pin::Pin;

/*
  Image file system archives can be very large, so we don't want to load the entire thing
  into memory in order to send it to Docker. Since `bollard::Docker::create_image` takes
  a `hyper::Body` struct, which can be created from a `futures_util::stream::Stream`, we will
  implement `Stream` on our own type `FileStreamer`.
*/
const BUFFER_SIZE: usize = 1048576; // 1 MB

struct FileStreamer {
    file: File,
    done: bool,
}

// just an example...
impl Stream for FileStreamer {
    type Item = IOResult<Vec<u8>>;

    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context) -> Poll<Option<Self::Item>> {
        if self.done {
            return Poll::Ready(None);
        }
        let mut buffer: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
        // The `std::fs::File.read` method here is blocking. For best results, use a non-blocking implementation
        match self.file.read(&mut buffer[..]) {
            Ok(BUFFER_SIZE) => Poll::Ready(Some(Ok(buffer.to_vec()))),
            // If less than `BUFFER_SIZE` bytes are read from the file, that means the whole file has been read.
            // The next time this stream is polled, return `None`.
            Ok(n) => {
                self.done = true;
                Poll::Ready(Some(Ok(buffer[0..n].to_vec())))
            }
            Err(e) => Poll::Ready(Some(Err(e))),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    let arguments: Vec<String> = args().collect();
    if arguments.len() == 1 || arguments.len() > 2 {
        println!("Usage: image_from_scratch <path to tar archive>");
        return Ok(());
    }

    let file = File::open(&arguments[1]).expect("Could not find archive.");

    let docker = Docker::connect_with_unix_defaults().unwrap();

    let options = CreateImageOptions {
        from_src: "-", // from_src must be "-" when sending the archive in the request body
        repo: "bollard_image_scratch_example", // The name of the image in the docker daemon.
        tag: "1.0.0",  // The tag of this particular image.
        ..Default::default()
    };
    // Create FileReader struct
    let reader = FileStreamer { file, done: false };
    // A `Body` can be created from a `Stream<Item = Result<...>>`
    let req_body: Body = Body::wrap_stream(reader);

    // Finally, call Docker::create_image with the options and the body
    let result: Vec<CreateImageInfo> = docker
        .create_image(Some(options), Some(req_body), None)
        .try_collect()
        .await?;
    // If all went well, the ID of the new image will be printed
    dbg!(&result[0]);

    Ok(())
}
