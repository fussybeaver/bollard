#[derive(Debug, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct FilesystemChange {
    pub Path: String,
    pub Kind: u8
}
