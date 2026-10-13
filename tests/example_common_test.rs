#[path = "../examples/common/mod.rs"]
mod common;

use std::io::Read;

#[test]
fn gzip_tar_preserves_paths_contents_and_modes() {
    let bytes = common::gzip_tar([
        common::TarFile::new("Dockerfile", b"FROM scratch\n".to_vec(), 0o644),
        common::TarFile::new("hello", b"binary payload".to_vec(), 0o755),
    ])
    .unwrap();

    let decoder = flate2::read::GzDecoder::new(bytes.as_slice());
    let mut archive = tar::Archive::new(decoder);
    let mut entries = archive.entries().unwrap();

    let mut dockerfile = entries.next().unwrap().unwrap();
    assert_eq!(dockerfile.path().unwrap().to_str(), Some("Dockerfile"));
    assert_eq!(dockerfile.header().mode().unwrap(), 0o644);
    let mut dockerfile_contents = Vec::new();
    dockerfile.read_to_end(&mut dockerfile_contents).unwrap();
    assert_eq!(dockerfile_contents, b"FROM scratch\n");

    let mut executable = entries.next().unwrap().unwrap();
    assert_eq!(executable.path().unwrap().to_str(), Some("hello"));
    assert_eq!(executable.header().mode().unwrap(), 0o755);
    let mut executable_contents = Vec::new();
    executable.read_to_end(&mut executable_contents).unwrap();
    assert_eq!(executable_contents, b"binary payload");
    assert!(entries.next().is_none());
}
