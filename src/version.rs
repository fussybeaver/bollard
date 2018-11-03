use arrayvec::ArrayVec;
use failure::Error;
use http::request::Builder;
use hyper::client::connect::Connect;
use hyper::rt::Future;
use hyper::{Body, Method};

use super::Docker;

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct Version {
    pub Version: String,
    pub ApiVersion: String,
    pub GitCommit: String,
    pub GoVersion: String,
    pub Os: String,
    pub Arch: String,
    pub KernelVersion: String,
    pub BuildTime: Option<String>,
    pub Experimental: Option<bool>,
}

impl<C> Docker<C>
where
    C: Connect + Sync + 'static,
{
    pub fn version(&self) -> impl Future<Item = Version, Error = Error> {
        let req = self.build_request2::<_, String, String>(
            "/version",
            Builder::new().method(Method::GET),
            Ok(None::<ArrayVec<[(_, _); 0]>>),
            Ok(Body::empty()),
        );

        self.process_into_value2(req)
    }
}
