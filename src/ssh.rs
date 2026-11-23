use crate::docker::SshOptions;
use futures_util::FutureExt;
use hyper_util::rt::TokioIo;
use std::future::Future;
use std::io;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{ready, Context, Poll};
use std::time::Duration;
use tokio::sync::{oneshot, Mutex, Notify, OwnedSemaphorePermit, Semaphore};

const DEFAULT_CHANNELS_PER_MASTER: usize = 8;
const DEFAULT_MASTERS_PER_DESTINATION: usize = 4;
const DEFAULT_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(15);
const MASTER_IDLE_SECONDS: usize = 60;
const MASTER_CHECK_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub(crate) struct SshConnector {
    shared: Arc<SshConnectorState>,
}

struct SshConnectorState {
    options: SshOptions,
    authority: String,
    pool: Arc<SshPool>,
}

impl SshConnector {
    pub fn new(options: SshOptions, authority: String) -> Self {
        Self {
            shared: Arc::new(SshConnectorState {
                options,
                authority,
                pool: Arc::new(SshPool::default()),
            }),
        }
    }
}

#[derive(Default)]
struct SshPool {
    state: Mutex<PoolState>,
    available: Notify,
}

#[derive(Default)]
struct PoolState {
    masters: Vec<Arc<Master>>,
    launching: usize,
}

struct Master {
    session: Arc<openssh::Session>,
    channels: Arc<Semaphore>,
}

struct ChannelLease {
    master: Arc<Master>,
    permit: Option<OwnedSemaphorePermit>,
    pool: Arc<SshPool>,
}

impl Drop for ChannelLease {
    fn drop(&mut self) {
        // Release capacity before waking another connector call.
        drop(self.permit.take());
        self.pool.available.notify_waiters();
    }
}

impl SshPool {
    async fn acquire(
        self: &Arc<Self>,
        builder: &openssh::SessionBuilder,
        destination: &str,
        channels_per_master: usize,
        max_masters: usize,
    ) -> Result<ChannelLease, openssh::Error> {
        loop {
            // Register before checking capacity to avoid losing a release notification.
            let available = self.available.notified();
            tokio::pin!(available);
            available.as_mut().enable();

            let mut state = self.state.lock().await;
            let selected = state.masters.iter().find_map(|master| {
                master
                    .channels
                    .clone()
                    .try_acquire_owned()
                    .ok()
                    .map(|permit| (Arc::clone(master), permit))
            });

            if let Some((master, permit)) = selected {
                drop(state);

                // An idle ControlPersist master may have exited since the last call.
                #[cfg(not(windows))]
                if !matches!(
                    tokio::time::timeout(MASTER_CHECK_TIMEOUT, master.session.check()).await,
                    Ok(Ok(()))
                ) {
                    self.retire(&master).await;
                    drop(permit);
                    continue;
                }

                return Ok(ChannelLease {
                    master,
                    permit: Some(permit),
                    pool: Arc::clone(self),
                });
            }

            if state.launching == 0 && state.masters.len() < max_masters {
                state.launching += 1;
                let (sender, receiver) = oneshot::channel();
                let pool = Arc::clone(self);
                let builder = builder.clone();
                let destination = destination.to_owned();
                tokio::spawn(async move {
                    let result = builder.launch_master(&destination).await.map(|tempdir| {
                        Arc::new(Master {
                            session: Arc::new(openssh::Session::new_process_mux(tempdir)),
                            channels: Arc::new(Semaphore::new(channels_per_master)),
                        })
                    });
                    let mut state = pool.state.lock().await;
                    state.launching -= 1;
                    if let Ok(master) = &result {
                        state.masters.push(Arc::clone(master));
                    }
                    drop(state);
                    pool.available.notify_waiters();
                    let _ = sender.send(result.map(|_| ()));
                });
                drop(state);

                match receiver.await {
                    Ok(Ok(())) => continue,
                    Ok(Err(error)) => return Err(error),
                    Err(_) => {
                        return Err(openssh::Error::Connect(io::Error::new(
                            io::ErrorKind::BrokenPipe,
                            "SSH master startup stopped unexpectedly",
                        )))
                    }
                }
            }

            drop(state);
            available.await;
        }
    }

    async fn retire(&self, master: &Arc<Master>) {
        let mut state = self.state.lock().await;
        state
            .masters
            .retain(|candidate| !Arc::ptr_eq(candidate, master));
        drop(state);
        self.available.notify_waiters();
    }
}

pub(crate) struct SshStream {
    _child: openssh::Child<Arc<openssh::Session>>,
    _channel: ChannelLease,
    stdin: Option<TokioIo<openssh::ChildStdin>>,
    stdout: TokioIo<openssh::ChildStdout>,
}

impl tower_service::Service<hyper::Uri> for SshConnector {
    type Response = SshStream;
    type Error = openssh::Error;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, destination: hyper::Uri) -> Self::Future {
        let shared = Arc::clone(&self.shared);

        async move {
            let options = shared.options.clone();
            let authority = match destination.scheme() {
                Some(scheme) if scheme == "ssh" => destination.authority().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "Missing authority")
                }),
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Invalid scheme {:?}", destination.scheme()),
                )),
            }
            .map_err(openssh::Error::Connect)?;

            if authority.as_str() != shared.authority {
                return Err(openssh::Error::Connect(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "SSH destination does not match the Docker client",
                )));
            }

            let channels_per_master = options
                .max_channels_per_master
                .map(NonZeroUsize::get)
                .unwrap_or(DEFAULT_CHANNELS_PER_MASTER);
            let max_masters = options
                .max_masters_per_destination
                .map(NonZeroUsize::get)
                .unwrap_or(DEFAULT_MASTERS_PER_DESTINATION);
            let acquire_timeout = options
                .channel_acquire_timeout
                .unwrap_or(DEFAULT_ACQUIRE_TIMEOUT);

            let mut builder = openssh::SessionBuilder::default();

            if let Some(key_path) = options.keypair_path {
                builder.keyfile(key_path);
            }
            if let Some(hosts_file) = options.user_known_hosts_file {
                builder.user_known_hosts_file(hosts_file);
            }
            if let Some(cfg_file) = options.config_file {
                builder.config_file(cfg_file);
            }
            if let Some(timeout) = options.connect_timeout {
                builder.connect_timeout(timeout);
            }
            if let Some(hosts_check) = options.known_hosts_check {
                if matches!(hosts_check, openssh::KnownHosts::Accept) {
                    log::warn!("host key verification is disabled, this may allow man-in-the-middle (MITM) attacks");
                }

                builder.known_hosts_check(hosts_check);
            }

            builder.control_persist(openssh::ControlPersist::IdleFor(
                NonZeroUsize::new(MASTER_IDLE_SECONDS).unwrap(),
            ));

            let resolved_uri = format!("ssh://{}", authority.as_str());
            let (builder, destination) = builder.resolve(&resolved_uri);
            let pool = &shared.pool;
            let channel = tokio::time::timeout(
                acquire_timeout,
                pool.acquire(&builder, destination, channels_per_master, max_masters),
            )
            .await
            .map_err(|_| {
                openssh::Error::Connect(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "timed out waiting for an SSH transport channel",
                ))
            })??;

            let child = Arc::clone(&channel.master.session)
                .arc_command("docker")
                .arg("system")
                .arg("dial-stdio")
                .stdin(openssh::Stdio::piped())
                .stdout(openssh::Stdio::piped())
                .spawn()
                .await;

            if child.is_err() {
                #[cfg(not(windows))]
                if !matches!(
                    tokio::time::timeout(MASTER_CHECK_TIMEOUT, channel.master.session.check()).await,
                    Ok(Ok(()))
                ) {
                    pool.retire(&channel.master).await;
                }
            }
            let mut child = child?;

            Ok(SshStream {
                stdin: Some(TokioIo::new(child.stdin().take().unwrap())),
                stdout: TokioIo::new(child.stdout().take().unwrap()),
                _child: child,
                _channel: channel,
            })
        }
        .boxed()
    }
}

impl SshStream {
    fn stdin(self: Pin<&mut Self>) -> io::Result<Pin<&mut TokioIo<openssh::ChildStdin>>> {
        self.get_mut()
            .stdin
            .as_mut()
            .map(Pin::new)
            .ok_or_else(|| io::ErrorKind::BrokenPipe.into())
    }

    fn stdout(self: Pin<&mut Self>) -> Pin<&mut TokioIo<openssh::ChildStdout>> {
        Pin::new(&mut self.get_mut().stdout)
    }
}

impl hyper::rt::Read for SshStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: hyper::rt::ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        self.stdout().poll_read(cx, buf)
    }
}

impl hyper::rt::Write for SshStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.stdin()?.poll_write(cx, buf)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        self.stdin()?.poll_write_vectored(cx, bufs)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.stdin()?.poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // currently, poll_shutdown does nothing.
        // https://github.com/tokio-rs/tokio/blob/b3a14483bf5efa1b5cf75af27f6ef0770f4c5689/tokio/src/process/unix/mod.rs#L314-L316
        ready!(self.as_mut().stdin()?.poll_shutdown(cx))?;
        // drop stdin to shutdown the input half.
        drop(self.get_mut().stdin.take());
        Poll::Ready(Ok(()))
    }
}

impl hyper_util::client::legacy::connect::Connection for SshStream {
    fn connected(&self) -> hyper_util::client::legacy::connect::Connected {
        hyper_util::client::legacy::connect::Connected::new()
    }
}

#[cfg(all(test, feature = "test_ssh"))]
mod tests {
    use super::*;
    use tower_service::Service;

    fn connector(mut options: SshOptions) -> (SshConnector, hyper::Uri) {
        let address = std::env::var("DOCKER_HOST").unwrap_or_else(|_| "ssh://localhost".into());
        if let Ok(key) = std::env::var("BOLLARD_TEST_SSH_KEY") {
            options.keypair_path = Some(key);
        }
        if let Ok(known_hosts) = std::env::var("BOLLARD_TEST_SSH_KNOWN_HOSTS") {
            options.user_known_hosts_file = Some(known_hosts);
            options.known_hosts_check = Some(openssh::KnownHosts::Strict);
        }
        let uri: hyper::Uri = address.parse().expect("valid SSH test address");
        let authority = uri.authority().expect("SSH test address has an authority");
        (SshConnector::new(options, authority.as_str().into()), uri)
    }

    #[tokio::test]
    async fn concurrent_channels_share_bounded_masters() {
        let (connector, uri) = connector(SshOptions::new());
        let calls = (0..16).map(|_| {
            let mut connector = connector.clone();
            let uri = uri.clone();
            async move { connector.call(uri).await.expect("SSH channel opens") }
        });
        let streams = futures_util::future::join_all(calls).await;

        assert_eq!(connector.shared.pool.state.lock().await.masters.len(), 2);
        drop(streams);
    }

    #[tokio::test]
    async fn exhausted_pool_times_out_and_recovers_after_drop() {
        let options = SshOptions::new()
            .with_max_channels_per_master(NonZeroUsize::new(1).unwrap())
            .with_max_masters_per_destination(NonZeroUsize::new(1).unwrap())
            .with_channel_acquire_timeout(Duration::from_secs(2));
        let (mut connector, uri) = connector(options);
        let stream = connector
            .call(uri.clone())
            .await
            .expect("first SSH channel");

        let error = connector
            .call(uri.clone())
            .await
            .err()
            .expect("pool is full");
        assert!(
            matches!(error, openssh::Error::Connect(ref io_error) if io_error.kind() == io::ErrorKind::TimedOut)
        );

        drop(stream);
        connector.call(uri).await.expect("channel released on drop");
    }

    #[tokio::test]
    async fn cancelled_caller_does_not_cancel_master_startup() {
        let (connector, uri) = connector(SshOptions::new());
        let mut caller = connector.clone();
        let task = tokio::spawn(async move { caller.call(uri).await });

        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if connector.shared.pool.state.lock().await.launching == 1 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("master startup began");

        task.abort();
        let _ = task.await;

        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let state = connector.shared.pool.state.lock().await;
                if state.launching == 0 && state.masters.len() == 1 {
                    break;
                }
                drop(state);
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("master startup completed after caller cancellation");
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn exited_master_is_replaced_before_next_channel() {
        let (mut connector, uri) = connector(SshOptions::new());
        let stream = connector
            .call(uri.clone())
            .await
            .expect("first SSH channel");
        let old = Arc::clone(&connector.shared.pool.state.lock().await.masters[0].session);
        drop(stream);

        let status = std::process::Command::new("ssh")
            .arg("-S")
            .arg(old.control_socket())
            .arg("-O")
            .arg("exit")
            .arg("localhost")
            .status()
            .expect("SSH control command runs");
        assert!(status.success());

        let stream = connector.call(uri).await.expect("replacement SSH channel");
        let state = connector.shared.pool.state.lock().await;
        assert_eq!(state.masters.len(), 1);
        assert!(!Arc::ptr_eq(&old, &state.masters[0].session));
        drop(state);
        drop(stream);
    }
}
