use crate::docker::SshOptions;
use futures_util::{
    future::{select, Either, Shared},
    FutureExt,
};
use hyper_util::rt::TokioIo;
use std::future::Future;
use std::io;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{ready, Context, Poll};
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::sync::{oneshot, Notify, OwnedSemaphorePermit, Semaphore, TryAcquireError};

const DEFAULT_CHANNELS_PER_MASTER: usize = 8;
const DEFAULT_MASTERS_PER_CLIENT: usize = 4;
const DEFAULT_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const MASTER_IDLE_SECONDS: usize = 60;
const MASTER_CHECK_TIMEOUT: Duration = Duration::from_secs(5);
const SERVER_ALIVE_INTERVAL: Duration = Duration::from_secs(15);

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
    masters: Vec<MasterEntry>,
    launching: Option<MasterStartup>,
}

type MasterStartup = Shared<oneshot::Receiver<Result<(), Arc<openssh::Error>>>>;

struct Master {
    session: Arc<openssh::Session>,
    channels: Arc<Semaphore>,
}

struct MasterEntry {
    master: Arc<Master>,
    retired: bool,
}

struct ChannelLease {
    master: Option<Arc<Master>>,
    runtime: Handle,
    permit: Option<OwnedSemaphorePermit>,
    pool: Arc<SshPool>,
}

impl Drop for ChannelLease {
    fn drop(&mut self) {
        let Some(master) = self.master.take() else {
            return;
        };
        let retired = {
            let mut state = self.pool.lock_state();
            let last_retired_lease = state.masters.iter().position(|entry| {
                entry.retired
                    && Arc::ptr_eq(&entry.master, &master)
                    && Arc::strong_count(&entry.master) == 2
            });
            // Keep lease references and permits in sync under the pool lock.
            drop(master);
            drop(self.permit.take());
            last_retired_lease.map(|index| state.masters.swap_remove(index).master)
        };
        if let Some(master) = retired {
            let runtime = Handle::try_current().unwrap_or_else(|_| self.runtime.clone());
            runtime.spawn_blocking(move || drop(master));
        }
        self.pool.available.notify_waiters();
    }
}

impl ChannelLease {
    fn master(&self) -> Result<&Arc<Master>, openssh::Error> {
        self.master.as_ref().ok_or_else(SshPool::unavailable)
    }
}

impl SshPool {
    fn unavailable() -> openssh::Error {
        openssh::Error::Connect(io::Error::other("SSH pool state is unavailable"))
    }

    fn lock_state(&self) -> MutexGuard<'_, PoolState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[cfg(not(windows))]
    fn retire(&self, master: &Arc<Master>) {
        let mut state = self.lock_state();
        if let Some(entry) = state
            .masters
            .iter_mut()
            .find(|entry| Arc::ptr_eq(&entry.master, master))
        {
            entry.retired = true;
        } else {
            log::error!("SSH pool cannot retire a missing master");
        }
        drop(state);
        self.available.notify_waiters();
    }

    fn start_master(
        self: &Arc<Self>,
        state: &mut PoolState,
        builder: openssh::SessionBuilder,
        destination: String,
        channels_per_master: usize,
    ) -> MasterStartup {
        let (sender, receiver) = oneshot::channel();
        let startup = receiver.shared();
        state.launching = Some(startup.clone());
        let pool = Arc::clone(self);
        tokio::spawn(async move {
            let result = builder.launch_master(&destination).await;
            if result.is_err() {
                log::warn!("SSH master startup failed");
            }
            let mut state = pool.lock_state();
            let outcome = result.map(|tempdir| {
                state.masters.push(MasterEntry {
                    master: Arc::new(Master {
                        session: Arc::new(openssh::Session::new_process_mux(tempdir)),
                        channels: Arc::new(Semaphore::new(channels_per_master)),
                    }),
                    retired: false,
                });
            });
            let _ = sender.send(outcome.map_err(Arc::new));
            state.launching = None;
            drop(state);
            pool.available.notify_waiters();
        });
        startup
    }

    async fn acquire(
        self: &Arc<Self>,
        builder: &openssh::SessionBuilder,
        destination: &str,
        channels_per_master: usize,
        max_masters: usize,
    ) -> Result<ChannelLease, openssh::Error> {
        loop {
            let available = self.available.notified();
            tokio::pin!(available);
            available.as_mut().enable();

            let (selected, startup) = {
                let mut state = self.lock_state();
                let selected = state
                    .masters
                    .iter()
                    .filter(|entry| !entry.retired)
                    .find_map(
                        |entry| match entry.master.channels.clone().try_acquire_owned() {
                            Ok(permit) => {
                                let was_idle = Arc::strong_count(&entry.master) == 1;
                                Some(Ok((Arc::clone(&entry.master), permit, was_idle)))
                            }
                            Err(TryAcquireError::NoPermits) => None,
                            Err(TryAcquireError::Closed) => {
                                log::error!("SSH pool channel semaphore closed unexpectedly");
                                Some(Err(Self::unavailable()))
                            }
                        },
                    )
                    .transpose()?;
                let live = state.masters.iter().filter(|entry| !entry.retired).count();
                let startup = if selected.is_some() {
                    None
                } else if let Some(startup) = &state.launching {
                    Some(startup.clone())
                } else if live < max_masters {
                    Some(self.start_master(
                        &mut state,
                        builder.clone(),
                        destination.to_owned(),
                        channels_per_master,
                    ))
                } else {
                    None
                };
                (selected, startup)
            };

            if let Some((master, permit, _was_idle)) = selected {
                let lease = ChannelLease {
                    master: Some(master),
                    runtime: Handle::current(),
                    permit: Some(permit),
                    pool: Arc::clone(self),
                };

                #[cfg(not(windows))]
                if _was_idle {
                    let master = lease.master()?;
                    let checked =
                        tokio::time::timeout(MASTER_CHECK_TIMEOUT, master.session.check()).await;
                    match checked {
                        Ok(Ok(())) => {}
                        Ok(Err(openssh::Error::Master(_) | openssh::Error::Disconnected)) => {
                            log::warn!("SSH master is unavailable; retiring it");
                            self.retire(master);
                            continue;
                        }
                        Ok(Err(_)) => {
                            log::warn!("SSH master check failed");
                            continue;
                        }
                        Err(_) => {
                            log::warn!("SSH master check timed out");
                            continue;
                        }
                    }
                }

                return Ok(lease);
            }

            if let Some(startup) = startup {
                if let Either::Left((outcome, _)) = select(startup, available).await {
                    match outcome {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => {
                            return Err(openssh::Error::Connect(io::Error::other(error)));
                        }
                        Err(_) => {
                            log::error!("SSH master startup task stopped unexpectedly");
                            return Err(openssh::Error::Connect(io::Error::new(
                                io::ErrorKind::BrokenPipe,
                                "SSH master startup stopped unexpectedly",
                            )));
                        }
                    }
                }
                continue;
            }

            available.await;
        }
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
                .unwrap_or(DEFAULT_CHANNELS_PER_MASTER)
                .min(Semaphore::MAX_PERMITS);
            let max_masters = options
                .max_masters_per_client
                .map(NonZeroUsize::get)
                .unwrap_or(DEFAULT_MASTERS_PER_CLIENT);
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
            builder.connect_timeout(options.connect_timeout.unwrap_or(DEFAULT_CONNECT_TIMEOUT));
            builder.server_alive_interval(SERVER_ALIVE_INTERVAL);
            if let Some(hosts_check) = options.known_hosts_check {
                if matches!(hosts_check, openssh::KnownHosts::Accept) {
                    log::warn!("host key verification is disabled, this may allow man-in-the-middle (MITM) attacks");
                }

                builder.known_hosts_check(hosts_check);
            }

            let idle_seconds = NonZeroUsize::new(MASTER_IDLE_SECONDS)
                .expect("SSH master idle timeout is nonzero");
            builder.control_persist(openssh::ControlPersist::IdleFor(idle_seconds));

            let resolved_uri = format!("ssh://{}", authority.as_str());
            let (builder, destination) = builder.resolve(&resolved_uri);
            let destination = destination
                .strip_prefix('[')
                .and_then(|host| host.strip_suffix(']'))
                .unwrap_or(destination);
            let pool = &shared.pool;
            let channel = tokio::time::timeout(
                acquire_timeout,
                pool.acquire(&builder, destination, channels_per_master, max_masters),
            )
            .await
            .map_err(|_| {
                openssh::Error::Connect(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "timed out waiting for a free SSH transport channel or SSH master startup",
                ))
            })??;

            let mut child = Arc::clone(&channel.master()?.session)
                .arc_command("docker")
                .arg("system")
                .arg("dial-stdio")
                .stdin(openssh::Stdio::piped())
                .stdout(openssh::Stdio::piped())
                .spawn()
                .await?;
            let stdin = child.stdin().take().ok_or_else(|| {
                log::error!("SSH Docker channel has no stdin pipe");
                openssh::Error::ChildIo(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "SSH Docker channel has no stdin pipe",
                ))
            })?;
            let stdout = child.stdout().take().ok_or_else(|| {
                log::error!("SSH Docker channel has no stdout pipe");
                openssh::Error::ChildIo(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "SSH Docker channel has no stdout pipe",
                ))
            })?;

            Ok(SshStream {
                stdin: Some(TokioIo::new(stdin)),
                stdout: TokioIo::new(stdout),
                _child: child,
                _channel: channel,
            })
        }
        .boxed()
    }
}

impl SshStream {
    fn stdin(&mut self) -> io::Result<Pin<&mut TokioIo<openssh::ChildStdin>>> {
        self.stdin
            .as_mut()
            .map(Pin::new)
            .ok_or_else(|| io::ErrorKind::BrokenPipe.into())
    }
}

impl hyper::rt::Read for SshStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: hyper::rt::ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().stdout).poll_read(cx, buf)
    }
}

impl hyper::rt::Write for SshStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.get_mut().stdin()?.poll_write(cx, buf)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        self.get_mut().stdin()?.poll_write_vectored(cx, bufs)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.get_mut().stdin()?.poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // currently, poll_shutdown does nothing.
        // https://github.com/tokio-rs/tokio/blob/b3a14483bf5efa1b5cf75af27f6ef0770f4c5689/tokio/src/process/unix/mod.rs#L314-L316
        ready!(self.as_mut().get_mut().stdin()?.poll_shutdown(cx))?;
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
    use crate::{Docker, API_DEFAULT_VERSION};
    use tower_service::Service;

    #[test]
    fn ssh_url_authority_is_normalized() {
        for (address, expected) in [
            ("ssh://user@example.com/", "user@example.com"),
            ("ssh://user@example.com:/", "user@example.com"),
            ("ssh://user@[0:0:0:0:0:0:0:1]:2222/", "user@[::1]:2222"),
            ("user@example.com:2222", "user@example.com:2222"),
        ] {
            let docker = Docker::connect_with_ssh_options(
                address,
                120,
                API_DEFAULT_VERSION,
                SshOptions::new(),
            )
            .expect("valid Docker SSH address");
            assert_eq!(docker.client_addr, expected);
        }
    }

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
    async fn poisoned_pool_recovers_and_opens_a_channel() {
        let (mut connector, uri) = connector(SshOptions::new());
        let pool = Arc::clone(&connector.shared.pool);
        let result = std::thread::spawn(move || {
            let _state = pool.lock_state();
            panic!("poison SSH pool state");
        })
        .join();
        assert!(result.is_err());

        connector.call(uri).await.expect("SSH pool recovers");
    }

    #[tokio::test(start_paused = true)]
    async fn acquisition_times_out_while_master_startup_is_pending() {
        let wait = Duration::from_secs(1);
        let (mut connector, uri) = connector(SshOptions::new().with_channel_acquire_timeout(wait));
        let (sender, receiver) = oneshot::channel();
        connector.shared.pool.state.lock().unwrap().launching = Some(receiver.shared());

        let started = tokio::time::Instant::now();
        let error = connector.call(uri).await.err().expect("startup is pending");
        assert!(matches!(error, openssh::Error::Connect(error)
                if error.kind() == io::ErrorKind::TimedOut
                    && error.to_string().contains("or SSH master startup")));
        assert_eq!(started.elapsed(), wait);
        assert!(!sender.is_closed());
    }

    #[tokio::test(start_paused = true)]
    async fn concurrent_acquirers_wait_for_the_same_startup_result() {
        let pool = Arc::new(SshPool::default());
        let (sender, receiver) = oneshot::channel();
        pool.state.lock().unwrap().launching = Some(receiver.shared());
        let builder = openssh::SessionBuilder::default();
        let mut first = Box::pin(pool.acquire(&builder, "unused", 1, 1));
        let mut second = Box::pin(pool.acquire(&builder, "unused", 1, 1));
        assert!(futures_util::poll!(&mut first).is_pending());
        assert!(futures_util::poll!(&mut second).is_pending());

        let failure = Arc::new(openssh::Error::Connect(io::Error::other(
            "shared startup failure",
        )));
        sender
            .send(Err(Arc::clone(&failure)))
            .expect("both callers are listening");
        for result in [first.await, second.await] {
            let error = result.err().expect("both callers get the startup failure");
            let openssh::Error::Connect(source) = error else {
                panic!("expected an SSH connection failure");
            };
            let cause = source
                .get_ref()
                .and_then(|error| error.downcast_ref::<Arc<openssh::Error>>())
                .expect("startup failure is preserved as the cause");
            assert!(Arc::ptr_eq(cause, &failure));
        }
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

        assert_eq!(connector.shared.pool.state.lock().unwrap().masters.len(), 2);
        drop(streams);
    }

    #[tokio::test]
    async fn exhausted_pool_times_out_and_recovers_after_drop() {
        let options = SshOptions::new()
            .with_max_channels_per_master(NonZeroUsize::new(1).unwrap())
            .with_max_masters_per_client(NonZeroUsize::new(1).unwrap())
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

    #[cfg(not(windows))]
    #[tokio::test]
    async fn retired_master_does_not_count_toward_limit() {
        let options = SshOptions::new().with_max_masters_per_client(NonZeroUsize::new(1).unwrap());
        let (mut connector, uri) = connector(options);
        let stream = connector
            .call(uri.clone())
            .await
            .expect("first SSH channel");
        let sibling = connector
            .call(uri.clone())
            .await
            .expect("shared SSH channel");
        let master = stream._channel.master().expect("stream has a master");
        let socket = master.session.control_socket().to_owned();
        connector.shared.pool.retire(master);

        let replacement = connector
            .call(uri)
            .await
            .expect("replacement SSH master opens");
        assert_eq!(connector.shared.pool.lock_state().masters.len(), 2);

        drop(stream);
        assert_eq!(connector.shared.pool.lock_state().masters.len(), 2);
        drop(sibling);
        assert_eq!(connector.shared.pool.lock_state().masters.len(), 1);
        tokio::time::timeout(Duration::from_secs(2), async {
            while socket.exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("retired master removed its control socket");
        drop(replacement);
    }

    #[tokio::test]
    async fn channel_limit_is_clamped_to_semaphore_capacity() {
        let options =
            SshOptions::new().with_max_channels_per_master(NonZeroUsize::new(usize::MAX).unwrap());
        let (mut connector, uri) = connector(options);
        let stream = connector.call(uri).await.expect("SSH channel opens");
        let state = connector.shared.pool.state.lock().unwrap();
        assert_eq!(
            state.masters[0].master.channels.available_permits(),
            Semaphore::MAX_PERMITS - 1
        );
        drop(state);
        drop(stream);
    }

    #[tokio::test]
    async fn cancelled_caller_does_not_cancel_master_startup() {
        let (connector, uri) = connector(SshOptions::new());
        let mut caller = connector.clone();
        let task = tokio::spawn(async move { caller.call(uri).await });

        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if connector
                    .shared
                    .pool
                    .state
                    .lock()
                    .unwrap()
                    .launching
                    .is_some()
                {
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
                let state = connector.shared.pool.state.lock().unwrap();
                if state.launching.is_none() && state.masters.len() == 1 {
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
        let old_socket = stream
            ._channel
            .master()
            .expect("stream has a master")
            .session
            .control_socket()
            .to_owned();
        drop(stream);

        let status = std::process::Command::new("ssh")
            .arg("-S")
            .arg(&old_socket)
            .arg("-O")
            .arg("exit")
            .arg("localhost")
            .status()
            .expect("SSH control command runs");
        assert!(status.success());

        tokio::time::timeout(Duration::from_secs(2), async {
            while old_socket.exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("old master removed its control socket");

        let stream = connector.call(uri).await.expect("replacement SSH channel");
        let state = connector.shared.pool.state.lock().unwrap();
        assert_eq!(state.masters.len(), 1);
        assert_ne!(old_socket, state.masters[0].master.session.control_socket());
        drop(state);
        drop(stream);
    }
}
