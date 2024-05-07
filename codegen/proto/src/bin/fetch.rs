use std::path::{Path, PathBuf};

struct Resource<'a> {
    destination: &'a str,
    source: &'a str,
    replacements: Vec<(&'a str, &'a str)>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let resources: Vec<Resource> = vec![
        Resource {
            destination: "fsutil/types/stat.proto",
            source: "https://raw.githubusercontent.com/tonistiigi/fsutil/master/types/stat.proto",
            replacements: vec![],
        },
        Resource {
            destination: "fsutil/types/wire.proto",
            source: "https://raw.githubusercontent.com/tonistiigi/fsutil/master/types/wire.proto",
            replacements: vec![
                ("stat.proto", "fsutil/types/stat.proto")
            ],
        },
        Resource {
            destination: "gogoproto/gogo.proto",
            source: "https://raw.githubusercontent.com/gogo/protobuf/master/gogoproto/gogo.proto",
            replacements: vec![]
        },
        Resource {
            destination: "google/protobuf/any.proto",
            source: "https://raw.githubusercontent.com/protocolbuffers/protobuf/main/src/google/protobuf/any.proto",
            replacements: vec![]
        },
        Resource {
            destination: "google/protobuf/descriptor.proto",
            source: "https://raw.githubusercontent.com/protocolbuffers/protobuf/main/src/google/protobuf/descriptor.proto",
            replacements: vec![]
        },
        Resource {
            destination: "google/protobuf/timestamp.proto",
            source: "https://raw.githubusercontent.com/protocolbuffers/protobuf/main/src/google/protobuf/timestamp.proto",
            replacements: vec![]
        },
        Resource {
            destination: "google/rpc/status.proto",
            source: "https://raw.githubusercontent.com/googleapis/googleapis/master/google/rpc/status.proto",
            replacements: vec![]
        },
        Resource {
            destination: "grpc/health/v1/health.proto",
            source: "https://raw.githubusercontent.com/grpc/grpc-proto/master/grpc/health/v1/health.proto",
            replacements: vec![]
        },
        Resource {
            destination: "moby/buildkit/v1/sourcepolicy/policy.proto",
            source: "https://raw.githubusercontent.com/moby/buildkit/master/sourcepolicy/pb/policy.proto",
            replacements: vec![]
        },
        Resource {
            destination: "moby/buildkit/v1/types/worker.proto",
            source: "https://raw.githubusercontent.com/moby/buildkit/master/api/types/worker.proto",
            replacements: vec![
                ("github.com/gogo/protobuf/gogoproto/gogo.proto", "gogoproto/gogo.proto"),
                ("github.com/moby/buildkit/solver/pb/ops.proto", "pb/ops.proto"),
            ]
        },
        Resource {
            destination: "moby/buildkit/v1/control.proto",
            source: "https://raw.githubusercontent.com/moby/buildkit/master/api/services/control/control.proto",
            replacements: vec![
                ("github.com/gogo/googleapis/google/rpc/status.proto", "google/rpc/status.proto"),
                ("github.com/gogo/protobuf/gogoproto/gogo.proto", "gogoproto/gogo.proto"),
                ("github.com/moby/buildkit/api/types/worker.proto", "moby/buildkit/v1/types/worker.proto"),
                ("github.com/moby/buildkit/solver/pb/ops.proto", "pb/ops.proto"),
                ("github.com/moby/buildkit/sourcepolicy/pb/policy.proto", "moby/buildkit/v1/sourcepolicy/policy.proto"),
            ]
        },
        Resource {
            destination: "moby/filesync/v1/auth.proto",
            source: "https://raw.githubusercontent.com/moby/buildkit/master/session/auth/auth.proto",
            replacements: vec![]
        },
        Resource {
            destination: "moby/filesync/v1/filesync.proto",
            source: "https://raw.githubusercontent.com/moby/buildkit/master/session/filesync/filesync.proto",
            replacements: vec![
               ("github.com/tonistiigi/fsutil/types/wire.proto", "fsutil/types/wire.proto") 
            ]
        },
        Resource {
            destination: "moby/upload/v1/upload.proto",
            source: "https://raw.githubusercontent.com/moby/buildkit/master/session/upload/upload.proto",
            replacements: vec![]
        },
        Resource {
            destination: "pb/ops.proto",
            source: "https://raw.githubusercontent.com/moby/buildkit/master/solver/pb/ops.proto",
            replacements: vec![
                ("github.com/gogo/protobuf/gogoproto/gogo.proto", "gogoproto/gogo.proto"),
            ]
        },
        Resource {
            destination: "moby/buildkit/v1/secrets.proto",
            source: "https://raw.githubusercontent.com/moby/buildkit/master/session/secrets/secrets.proto",
            replacements: vec![]
        },
        Resource {
            destination: "moby/buildkit/v1/ssh.proto",
            source: "https://raw.githubusercontent.com/moby/buildkit/master/session/sshforward/ssh.proto",
            replacements: vec![]
        },
    ];

    let resources_dir = std::env::current_dir().expect("Cannot determine current directory");
    let target_dir = Path::join(&PathBuf::from(&resources_dir), PathBuf::from("resources"));

    if let Err(_) = std::path::Path::try_exists(&target_dir) {
        std::fs::create_dir_all(&target_dir).expect("Cannot create temporary directory")
    }

    for resource in resources {
        let resource_path = Path::new(resource.destination);
        let resource_dir = resource_path
            .parent()
            .expect("Cannot retrieve dirname for resource");
        let abs_resource_dir = Path::join(&target_dir, resource_dir);
        std::fs::create_dir_all(abs_resource_dir).expect("Cannot create resource directory");
        let response = ureq::get(resource.source)
            .call()
            .expect("Cannot fetch resource URL");
        let abs_resource_file = Path::join(&target_dir, resource.destination);
        let mut src = response
            .into_string()
            .expect("Cannot create UTF8 string from HTTP response");
        for replacement in resource.replacements {
            src.find(replacement.0).expect(
                format!(
                    "Expected to find {} in {}",
                    replacement.0, &resource.destination
                )
                .as_str(),
            );
            src = src.replace(replacement.0, replacement.1);
        }
        std::fs::write(abs_resource_file, &src).expect("Cannot write resource file");
    }

    Ok(())
}
