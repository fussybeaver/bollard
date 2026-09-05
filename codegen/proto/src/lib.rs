#![allow(missing_docs, unused_qualifications)]

pub mod fsutil {
    pub mod types {
        include!("generated/fsutil.types.rs");
    }
}

pub mod google {
    pub mod protobuf {
        include!("generated/google.protobuf.rs");
    }
    pub mod rpc {
        include!("generated/google.rpc.rs");
    }
}

pub mod health {
    include!("generated/grpc.health.v1.rs");
}

pub mod provenance {
    include!("generated/provenance.rs");
}

pub mod moby {
    pub mod buildkit {
        pub mod secrets {
            pub mod v1 {
                include!("generated/moby.buildkit.secrets.v1.rs");
            }
        }
        pub mod v1 {
            include!("generated/moby.buildkit.v1.rs");
            pub mod sourcepolicy {
                include!("generated/moby.buildkit.v1.sourcepolicy.rs");
            }
            pub mod types {
                include!("generated/moby.buildkit.v1.types.rs");
            }
        }
    }
    pub mod filesync {
        pub mod v1 {
            include!("generated/moby.filesync.v1.rs");
        }
        pub mod packet {
            include!("generated/moby.filesync.packet.rs");
        }
    }
    pub mod upload {
        pub mod v1 {
            include!("generated/moby.upload.v1.rs");
        }
    }
    pub mod sshforward {
        pub mod v1 {
            include!("generated/moby.sshforward.v1.rs");
        }
    }
}

#[allow(clippy::all)]
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

impl AsRef<[u8]> for moby::buildkit::v1::BytesMessage {
    fn as_ref(&self) -> &[u8] {
        self.data.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use prost::Message;

    use super::provenance;

    #[test]
    fn generated_default_image_matches_buildkit_version() {
        assert_eq!(
            provenance::DEFAULT_IMAGE,
            format!("moby/buildkit:{}", provenance::BUILDKIT_VERSION)
        );
        assert_eq!(provenance::BUILDKIT_COMMIT.len(), 40);
        assert_eq!(provenance::OPS_PROTO_SHA256.len(), 64);
        assert!(provenance::MOBY_TAG.starts_with("docker-v"));
    }

    #[test]
    fn llb_definition_is_available_without_grpc() {
        let definition = super::pb::Definition {
            def: vec![vec![1, 2, 3]],
            ..Default::default()
        };

        let encoded = definition.encode_to_vec();
        assert_eq!(super::pb::Definition::decode(encoded.as_slice()), Ok(definition));
    }
}
