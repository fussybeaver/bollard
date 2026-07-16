#![cfg(feature = "buildkit")]

use bollard::errors::Error;
use bollard::grpc::driver::{DefinitionExporter, DefinitionSolveRequest, SolveDefinition};
use bollard::Docker;
use bollard_buildkit_proto::pb;
use prost::Message;
use tokio::runtime::Runtime;

#[macro_use]
pub mod common;
use crate::common::*;

const MKFILE_GOLDEN: &[u8] = include_bytes!("../llb/testdata/golden/mkfile.llb.pb");

async fn llb_solve_mkfile_test(docker: Docker) -> Result<(), Error> {
    let definition = pb::Definition::decode(MKFILE_GOLDEN).map_err(|e| Error::IOError {
        err: std::io::Error::other(format!("failed to decode mkfile golden: {e}")),
    })?;

    let dest = tempfile::tempdir().map_err(|e| Error::IOError { err: e })?;

    let driver = crate::common::buildkit_test::builder(&docker)
        .bootstrap()
        .await
        .unwrap();

    let version_record = crate::common::buildkit_test::record_version(&docker, &driver).await;
    if let Ok(record) = version_record.as_ref() {
        println!("{}", record);
    }

    let request = DefinitionSolveRequest::new(
        definition,
        DefinitionExporter::Local(dest.path().to_path_buf()),
    );

    let res = SolveDefinition::solve_definition(driver, request).await;
    assert!(res.is_ok(), "solve_definition failed: {res:?}");

    version_record?;

    let hello = dest.path().join("hello");
    assert!(hello.exists(), "exported /hello should exist");

    let content = std::fs::read(&hello).map_err(|e| Error::IOError { err: e })?;
    assert_eq!(content, b"world", "unexpected /hello contents");

    Ok(())
}

#[test]
#[cfg(feature = "buildkit_providerless")]
fn integration_test_llb_solve_mkfile() {
    connect_to_docker_and_run!(llb_solve_mkfile_test);
}

#[test]
#[cfg(feature = "buildkit_providerless")]
fn bollard_llb_definition_to_pb_boundary() {
    use std::path::PathBuf;

    use bollard_llb::{mkfile, scratch, FileOpts, MarshalOpts};

    let def = scratch()
        .unwrap()
        .file(mkfile("/hello", 0o644, b"world"), FileOpts::new())
        .unwrap()
        .marshal(MarshalOpts::linux_amd64())
        .unwrap()
        .to_pb();

    let _request =
        DefinitionSolveRequest::new(def, DefinitionExporter::Local(PathBuf::from("/out")));
}
