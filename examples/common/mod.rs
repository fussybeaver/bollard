//! Helpers shared by examples that send a tarred Docker build context.

use std::io::Write;

/// One file to include in a generated build-context archive.
pub struct TarFile {
    path: String,
    contents: Vec<u8>,
    mode: u32,
}

impl TarFile {
    /// Create a file entry from its archive path, contents, and permission bits.
    pub fn new(path: impl Into<String>, contents: impl Into<Vec<u8>>, mode: u32) -> Self {
        Self {
            path: path.into(),
            contents: contents.into(),
            mode,
        }
    }
}

/// Build a gzip-compressed tar archive from file entries.
pub fn gzip_tar(
    files: impl IntoIterator<Item = TarFile>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut tar = tar::Builder::new(Vec::new());
    for file in files {
        let mut header = tar::Header::new_gnu();
        header.set_path(file.path)?;
        header.set_size(file.contents.len() as u64);
        header.set_mode(file.mode);
        header.set_cksum();
        tar.append(&header, file.contents.as_slice())?;
    }

    let uncompressed = tar.into_inner()?;
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(&uncompressed)?;
    Ok(gzip.finish()?)
}
