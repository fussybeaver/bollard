#![allow(missing_docs, unused_qualifications)]
#![cfg(not(feature = "build"))]

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
}

pub mod google {
    pub use prost_types as protobuf;
}

pub mod pb {
    include!("generated/pb.rs");
}
