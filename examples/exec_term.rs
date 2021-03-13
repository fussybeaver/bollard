//! This example will run a interactive command inside the container using `docker exec`,
//! passing trough input and output into the tty running inside the container

use bollard::container::{Config, RemoveContainerOptions};
use bollard::Docker;

use bollard::exec::{CreateExecOptions, StartExecResults};
use std::io::{stdout, Read, Write};
use std::time::Duration;
use termion::async_stdin;
use termion::raw::IntoRawMode;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::task::spawn;
use tokio::time::sleep;

const IMAGE: &'static str = "alpine:3";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    let docker = Docker::connect_with_unix_defaults().unwrap();

    let alpine_config = Config {
        image: Some(IMAGE),
        tty: Some(true),
        ..Default::default()
    };

    let id = docker
        .create_container::<&str, &str>(None, alpine_config)
        .await?
        .id;
    docker.start_container::<String>(&id, None).await?;

    let exec = docker
        .create_exec(
            &id,
            CreateExecOptions {
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                attach_stdin: Some(true),
                tty: Some(true),
                cmd: Some(vec!["sh"]),
                ..Default::default()
            },
        )
        .await?
        .id;
    if let StartExecResults::AttachedTTY {
        mut output,
        mut input,
    } = docker.start_exec(&exec, None, true).await?
    {
        // pipe stdin into the docker exec stream input
        spawn(async move {
            let mut stdin = async_stdin().bytes();
            loop {
                if let Some(Ok(byte)) = stdin.next() {
                    input.write(&[byte]).await.ok();
                } else {
                    sleep(Duration::from_nanos(10)).await;
                }
            }
        });

        // set stdout in raw mode so we can do tty stuff
        let stdout = stdout();
        let mut stdout = stdout.lock().into_raw_mode()?;

        // pipe docker exec output into stdout
        let mut buff = [0; 128];
        while let Ok(read) = output.read(&mut buff).await {
            if read == 0 {
                break;
            }
            stdout.write(&buff[0..read])?;
            stdout.flush()?;
        }
    }

    docker
        .remove_container(
            &id,
            Some(RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await?;

    Ok(())
}
