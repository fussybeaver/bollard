extern crate bollard;
extern crate futures;
extern crate hyper;
extern crate tokio;
extern crate tokio_reactor;
extern crate tokio_threadpool;
extern crate tokio_timer;

use bollard::system::Version;
use bollard::{ClientVersion, Docker};
use futures::Async;
use hyper::rt::Future;
use tokio::executor::thread_pool;
use tokio::reactor;
use tokio::runtime::Runtime;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[macro_use]
mod common;

#[cfg(windows)]
#[test]
fn test_version_named_pipe() {
    rt_exec!(
        Docker::connect_with_named_pipe_defaults()
            .unwrap()
            .version(),
        |version: Version| assert_eq!(version.os, "windows")
    )
}

#[cfg(all(unix, not(feature = "test_http"), not(feature = "ssl")))]
#[test]
fn test_version_unix() {
    rt_exec!(
        Docker::connect_with_unix_defaults().unwrap().version(),
        |version: Version| assert_eq!(version.os, "linux")
    )
}

#[cfg(feature = "ssl")]
#[test]
fn test_version_ssl() {
    rt_exec!(
        Docker::connect_with_ssl_defaults().unwrap().version(),
        |version: Version| assert_eq!(version.os, "linux")
    )
}

#[cfg(feature = "test_http")]
#[test]
fn test_version_http() {
    #[cfg(unix)]
    rt_exec!(
        Docker::connect_with_http_defaults().unwrap().version(),
        |version: Version| assert_eq!(version.os, "linux")
    );
    #[cfg(windows)]
    rt_exec!(
        Docker::connect_with_http_defaults().unwrap().version(),
        |version: Version| assert_eq!(version.os, "windows")
    )
}

#[cfg(feature = "test_tls")]
#[test]
fn test_version_tls() {
    rt_exec!(
        Docker::connect_with_tls_defaults().unwrap().version(),
        |version: Version| assert_eq!(version.os, "linux")
    )
}

#[cfg(all(unix, not(feature = "test_http"), not(feature = "ssl")))]
#[test]
// This sometimes locks up in CircleCI
#[ignore]
fn test_threadpool() {
    let reactor = reactor::Reactor::new().unwrap();

    let mut pool_builder = thread_pool::Builder::new();
    let handle = reactor.handle();

    let timers = Arc::new(Mutex::new(HashMap::<_, ::tokio_timer::timer::Handle>::new()));
    let t1 = timers.clone();

    pool_builder
        .around_worker(move |w, enter| {
            let timer_handle = t1.lock().unwrap().get(w.id()).unwrap().clone();

            ::tokio_reactor::with_default(&handle, enter, |enter| {
                ::tokio_timer::timer::with_default(&timer_handle, enter, |_| {
                    w.run();
                });
            });
        })
        .custom_park(move |worker_id| {
            // Create a new timer
            let timer =
                ::tokio_timer::timer::Timer::new(::tokio_threadpool::park::DefaultPark::new());

            timers
                .lock()
                .unwrap()
                .insert(worker_id.clone(), timer.handle());

            timer
        });

    pool_builder.pool_size(4);
    let pool = pool_builder.build();

    struct Boom;
    impl Future for Boom {
        type Item = ();
        type Error = ();
        fn poll(&mut self) -> Result<Async<Self::Item>, Self::Error> {
            Docker::connect_with_unix_defaults()
                .unwrap()
                .version()
                .wait()
                .map(|v| {
                    println!("{:?}", v);
                    Async::Ready(())
                })
                .map_err(|e| panic!("{:?}", e))
        }
    }

    impl Drop for Boom {
        fn drop(&mut self) {
            assert!(!::std::thread::panicking());
        }
    }

    pool.spawn(Boom);

    let _bg = reactor.background();

    pool.shutdown_on_idle().wait().unwrap();
}

#[cfg(unix)]
#[test]
fn test_downversioning() {
    let mut rt = Runtime::new().unwrap();

    env_logger::init();

    let docker = Docker::connect_with_unix(
        "unix:///var/run/docker.sock",
        120,
        &ClientVersion {
            major_version: 1,
            minor_version: 24,
        },
    )
    .unwrap();

    let future = docker.negotiate_version().map(|docker| {
        docker.version();
        docker
    });

    let docker = rt.block_on(future).unwrap();
    assert_eq!(
        format!("{}", docker.client_version()),
        format!("{}", "1.24")
    );
    rt.shutdown_now().wait().unwrap();
}
