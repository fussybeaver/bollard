#[derive(RustcEncodable, RustcDecodable)]
#[allow(non_snake_case)]
pub struct Image {
    pub Created: u64,
    pub Id: String,
    pub ParentId: String,
    pub RepoTags: Vec<String>,
    pub Size: u64,
    pub VirtualSize: u64
}
