#![allow(missing_docs, unused_qualifications)]
#![cfg(not(feature = "build"))]

pub mod fsutil {
    pub mod types {
        include!("generated/fsutil.types.rs");
    }
}

pub mod health {
    include!("generated/grpc.health.v1.rs");
}

pub mod moby {
    pub mod buildkit {
        pub mod v1 {
            include!("generated/moby.buildkit.v1.rs");
            pub mod types {
                include!("generated/moby.buildkit.v1.types.rs");
            }
        }
    }
    pub mod filesync {
        pub mod v1 {
            include!("generated/moby.filesync.v1.rs");
        }
    }
    pub mod upload {
        pub mod v1 {
            include!("generated/moby.upload.v1.rs");
        }
    }

}

pub mod google {
    pub use prost_types as protobuf;
}

pub mod pb {
    include!("generated/pb.rs");
}

use std::fmt::{self, Display, Formatter};

impl Display for moby::buildkit::v1::StatusResponse {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "StatusResponse: {{ vertexes: {:?}, statuses: {:?}, logs: ",
            self.vertexes, self.statuses
        )
        .and_then(|_| {
            if self.logs.is_empty() {
                write!(f, "[]")
            } else {
                let mut iter = self.logs.iter().peekable();
                let mut next = iter.next();
                let mut result = Ok(());
                while next.is_some() {

                    result = result.and_then(|_| write!(f, "{}", next.unwrap()));
                    next = iter.next();
                    if iter.peek().is_some() {
                        result = result.and_then(|_| write!(f, ", "));
                    }
                }
                result
            }
        })
        .and_then(|_| write!(f, r#", warnings: {:?} }}"#, self.warnings))
    }
}

impl Display for moby::buildkit::v1::VertexLog {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            r#"VertexLog: {{ vertex: {:?}, timestamp: {:?}, stream: {:?}, msg: \"{}\" }}"#,
            self.vertex,
            self.timestamp,
            self.stream,
            String::from_utf8_lossy(&self.msg).trim(),
        )
    }
}
