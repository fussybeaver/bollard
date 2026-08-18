use crate::support::{xtask_error as transform_error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transform {
    None,
    BuildkitControlImports,
    BuildkitWorkerImports,
    FsutilStatImports,
    FsutilWireImports,
    FilesyncImports,
    FilesyncPacket,
}

impl Transform {
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::BuildkitControlImports => "rewrite-buildkit-control-imports",
            Self::BuildkitWorkerImports => "rewrite-buildkit-worker-imports",
            Self::FsutilStatImports => "rewrite-fsutil-stat-imports",
            Self::FsutilWireImports => "rewrite-fsutil-wire-imports",
            Self::FilesyncImports => "rewrite-filesync-imports",
            Self::FilesyncPacket => "adapt-filesend-packet",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Replacement {
    from: &'static str,
    to: &'static str,
    expected: usize,
}

const BUILDKIT_CONTROL_IMPORTS: &[Replacement] = &[
    Replacement {
        from: "github.com/moby/buildkit/api/types/worker.proto",
        to: "moby/buildkit/v1/types/worker.proto",
        expected: 1,
    },
    Replacement {
        from: "github.com/moby/buildkit/solver/pb/ops.proto",
        to: "pb/ops.proto",
        expected: 1,
    },
    Replacement {
        from: "github.com/moby/buildkit/sourcepolicy/pb/policy.proto",
        to: "moby/buildkit/v1/sourcepolicy/policy.proto",
        expected: 1,
    },
];

const BUILDKIT_WORKER_IMPORTS: &[Replacement] = &[Replacement {
    from: "github.com/moby/buildkit/solver/pb/ops.proto",
    to: "pb/ops.proto",
    expected: 1,
}];

const FSUTIL_STAT_IMPORTS: &[Replacement] = &[Replacement {
    from: "github.com/planetscale/vtprotobuf/vtproto/ext.proto",
    to: "vtproto/vtproto/ext.proto",
    expected: 1,
}];

const FSUTIL_WIRE_IMPORTS: &[Replacement] = &[
    Replacement {
        from: "github.com/tonistiigi/fsutil/types/stat.proto",
        to: "fsutil/types/stat.proto",
        expected: 1,
    },
    Replacement {
        from: "github.com/planetscale/vtprotobuf/vtproto/ext.proto",
        to: "vtproto/vtproto/ext.proto",
        expected: 1,
    },
];

const FILESYNC_IMPORTS: &[Replacement] = &[Replacement {
    from: "github.com/tonistiigi/fsutil/types/wire.proto",
    to: "fsutil/types/wire.proto",
    expected: 1,
}];

const FILESYNC_PACKET: &[Replacement] = &[
    Replacement {
        from: "github.com/tonistiigi/fsutil/types/wire.proto",
        to: "fsutil/types/wire.proto",
        expected: 1,
    },
    Replacement {
        from: "service FileSend{\n\trpc DiffCopy(stream BytesMessage) returns (stream BytesMessage);\n}",
        to: "service FileSend{\n\trpc DiffCopy(stream fsutil.types.Packet) returns (stream fsutil.types.Packet);\n}",
        expected: 1,
    },
];

fn replacements(transform: Transform) -> &'static [Replacement] {
    match transform {
        Transform::None => &[],
        Transform::BuildkitControlImports => BUILDKIT_CONTROL_IMPORTS,
        Transform::BuildkitWorkerImports => BUILDKIT_WORKER_IMPORTS,
        Transform::FsutilStatImports => FSUTIL_STAT_IMPORTS,
        Transform::FsutilWireImports => FSUTIL_WIRE_IMPORTS,
        Transform::FilesyncImports => FILESYNC_IMPORTS,
        Transform::FilesyncPacket => FILESYNC_PACKET,
    }
}

pub fn for_destination(destination: &str) -> Transform {
    match destination {
        "moby/buildkit/v1/control.proto" => Transform::BuildkitControlImports,
        "moby/buildkit/v1/types/worker.proto" => Transform::BuildkitWorkerImports,
        "fsutil/types/stat.proto" => Transform::FsutilStatImports,
        "fsutil/types/wire.proto" => Transform::FsutilWireImports,
        "moby/filesync/v1/filesync.proto" => Transform::FilesyncImports,
        "moby/filesync/v1/filesync.packet.proto" => Transform::FilesyncPacket,
        _ => Transform::None,
    }
}

pub fn apply(transform: Transform, destination: &str, contents: &[u8]) -> Result<Vec<u8>> {
    if transform == Transform::None {
        return Ok(contents.to_vec());
    }

    let mut output = String::from_utf8(contents.to_vec()).map_err(|error| {
        transform_error(format!(
            "{} cannot transform {destination}: source is not UTF-8: {error}",
            transform.name()
        ))
    })?;

    for replacement in replacements(transform) {
        let actual = output.matches(replacement.from).count();
        if actual != replacement.expected {
            return Err(transform_error(format!(
                "{} cannot transform {destination}: expected {} matches for {:?}, found {}",
                transform.name(), replacement.expected, replacement.from, actual
            )));
        }
        output = output.replace(replacement.from, replacement.to);
    }

    Ok(output.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::{apply, for_destination, Transform};

    #[test]
    fn identity_transform_preserves_bytes() {
        let input = b"not utf8 is okay for no transform: \xff";
        assert_eq!(apply(Transform::None, "none.proto", input).unwrap(), input);
    }

    #[test]
    fn rewrites_buildkit_control_imports() {
        let input = concat!(
            "import \"github.com/moby/buildkit/api/types/worker.proto\";\n",
            "import \"github.com/moby/buildkit/solver/pb/ops.proto\";\n",
            "import \"github.com/moby/buildkit/sourcepolicy/pb/policy.proto\";\n",
        );
        let output = apply(
            Transform::BuildkitControlImports,
            "control.proto",
            input.as_bytes(),
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            concat!(
                "import \"moby/buildkit/v1/types/worker.proto\";\n",
                "import \"pb/ops.proto\";\n",
                "import \"moby/buildkit/v1/sourcepolicy/policy.proto\";\n",
            )
        );
    }

    #[test]
    fn rewrites_filesync_packet_without_touching_filesync_rpc() {
        let input = concat!(
            "import \"github.com/tonistiigi/fsutil/types/wire.proto\";\n",
            "service FileSync{\n",
            "\trpc DiffCopy(stream fsutil.types.Packet) returns (stream fsutil.types.Packet);\n",
            "}\n",
            "service FileSend{\n",
            "\trpc DiffCopy(stream BytesMessage) returns (stream BytesMessage);\n",
            "}\n",
        );
        let output = apply(Transform::FilesyncPacket, "packet.proto", input.as_bytes()).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert_eq!(output.matches("fsutil/types/wire.proto").count(), 1);
        assert_eq!(output.matches("stream fsutil.types.Packet").count(), 4);
        assert!(!output.contains("stream BytesMessage) returns (stream BytesMessage)"));
    }

    #[test]
    fn rejects_missing_expected_match() {
        let error = apply(
            Transform::BuildkitWorkerImports,
            "worker.proto",
            b"import \"pb/ops.proto\";\n",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("rewrite-buildkit-worker-imports"));
        assert!(error.contains("worker.proto"));
        assert!(error.contains("expected 1"));
        assert!(error.contains("found 0"));
    }

    #[test]
    fn rejects_duplicate_expected_match() {
        let error = apply(
            Transform::BuildkitWorkerImports,
            "worker.proto",
            b"github.com/moby/buildkit/solver/pb/ops.proto\ngithub.com/moby/buildkit/solver/pb/ops.proto",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("expected 1"));
        assert!(error.contains("found 2"));
    }

    #[test]
    fn maps_only_known_destinations_to_transforms() {
        assert_eq!(
            for_destination("moby/filesync/v1/filesync.packet.proto"),
            Transform::FilesyncPacket
        );
        assert_eq!(for_destination("pb/ops.proto"), Transform::None);
    }
}
