extern crate boondock;
extern crate hyper;
extern crate hyperlocal;
extern crate tokio;

use boondock::Docker;
use hyper::rt::Future;
use hyperlocal::UnixConnector;
use tokio::runtime::Runtime;

fn main() {
    let mut rt = Runtime::new().unwrap();

    // --

    let docker = Docker::<UnixConnector>::new().unwrap();
    let f = docker
        .version()
        .map(|version| println!("version: {:#?}", version))
        .map_err(|err| println!("error: {}, backtrace: {}", err, err.backtrace()));

    // --

    rt.block_on(f).unwrap();
    rt.shutdown_now().wait().unwrap();
}
