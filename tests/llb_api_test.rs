#![cfg(feature = "buildkit_providerless")]

use bollard::llb::{self, FileOpts, MarshalOpts};

#[test]
fn public_llb_api_marshals_bollard_definition() {
    let state = llb::scratch()
        .unwrap()
        .file(llb::mkfile("/hello", 0o644, b"world"), FileOpts::new())
        .unwrap();

    let graph = state.marshal(MarshalOpts::linux_amd64()).unwrap();
    let definition: bollard::pb::Definition = graph.to_pb();

    assert!(!definition.def.is_empty());
}
