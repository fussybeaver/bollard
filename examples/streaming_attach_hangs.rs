use bollard::container::{AttachContainerOptions, Config, LogOutput, WaitContainerOptions};
use bollard::image::CreateImageOptions;
use bollard::Docker;
use bytes::BytesMut;
use futures_util::StreamExt;

async fn pull_image(docker: &Docker, image: &str) -> Result<(), String> {
    let create_image_options = CreateImageOptions::<String> {
        from_image: image.to_string(),
        ..CreateImageOptions::default()
    };

    let mut result_stream = docker.create_image(Some(create_image_options), None, None);
    while let Some(msg) = result_stream.next().await {
        if let Err(err) = msg {
            return Err(format!(
                "Failed to pull Docker image `{}`: {:?}",
                image, err
            ));
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    let docker = Docker::connect_with_local_defaults().expect("connect to docker");

    const IMAGE: &str = "busybox:1";

    // pull
    pull_image(&docker, IMAGE).await.expect("pull");

    // create container
    // equivalent to: `docker run busybox:1 env`
    let config = Config {
        cmd: Some(vec!["env"]),
        image: Some(IMAGE),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        ..Config::default()
    };
    let container = docker
        .create_container::<&str, &str>(None, config)
        .await
        .map_err(|err| format!("Failed to create Docker container: {:?}", err))
        .expect("create container");

    println!("created container `{}`", &container.id);

    // start container
    docker
        .start_container::<String>(&container.id, None)
        .await
        .map_err(|err| {
            format!(
                "Failed to start Docker container `{}`: {:?}",
                &container.id, err
            )
        })
        .expect("start container");

    println!("started container");

    // attach to container
    let attach_options = AttachContainerOptions::<String> {
        stdout: Some(true),
        stderr: Some(true),
        logs: Some(true), // stream any output that was missed between the start_container call and now
        stream: Some(true),
        ..AttachContainerOptions::default()
    };
    let attach_result = docker
        .attach_container(&container.id, Some(attach_options))
        .await
        .map_err(|err| {
            format!(
                "Failed to attach to Docker container `{}`: {:?}",
                &container.id, err
            )
        })
        .expect("attach to container");

    println!("attached to container");

    let mut output_stream = attach_result.output.boxed();

    let wait_options = WaitContainerOptions {
        condition: "not-running",
    };
    let mut wait_stream = docker
        .wait_container(&container.id, Some(wait_options))
        .boxed();

    //
    // container monitoring loop
    //

    let mut status_code: Option<i32> = None;
    let mut stdout = BytesMut::new();
    let mut stderr = BytesMut::new();

    loop {
        // Read from each stream and append output to correct buffer,.
        tokio::select! {
          // Monitor for stdout/stderr output events.
          Some(output_msg) = output_stream.next() => {
            match output_msg {
              Ok(LogOutput::StdOut { message }) => {
                println!("container wrote {} bytes to stdout", message.len());
                stdout.extend(message);
              }
              Ok(LogOutput::StdErr { message }) => {
                println!("container wrote {} bytes to stderr", message.len());
                stderr.extend(message);
              }
              _ => (),
            }
          }

          // Monitor for container exit.
          Some(wait_msg) = wait_stream.next() => {
            println!("wait_container stream: {:?}", wait_msg);
            match wait_msg {
              Ok(r) => {
                // Set the status_code but do not emit an event yet. This will allow collecting
                // any remaining output that might remain on `output_stream`.
                status_code = Some(r.status_code as i32);
                break;
              }
              Err(err) => {
                println!("wait_container stream error: {:?}", err);
                break;
              }
            }
          }
        }
    }

    println!("primary monitoring loop ended, checking for remaining output");

    // Note that there still may be items to read from `output_stream`.
    while let Some(output_msg) = output_stream.next().await {
        match output_msg {
            Ok(LogOutput::StdOut { message }) => {
                println!("container wrote {} bytes to stdout", message.len());
                stdout.extend(message);
            }
            Ok(LogOutput::StdErr { message }) => {
                println!("container wrote {} bytes to stderr", message.len());
                stderr.extend(message);
            }
            Ok(_) => (),
            Err(err) => {
                println!("error during final output processing: {err}")
            }
        }
    }

    println!("finished collecting output for container {}", &container.id);

    // dump the container's output
    let status_code = status_code.expect("got status code");
    let stdout = stdout.freeze();
    let stdout = String::from_utf8_lossy(&stdout);
    let stderr = stderr.freeze();
    let stderr = String::from_utf8_lossy(&stderr);
    println!("status = {status_code}");
    println!("stdout:\n{stdout}\nstderr:\n{stderr}");
}
