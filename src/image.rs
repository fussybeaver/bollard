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

impl Clone for Image {
    fn clone(&self) -> Self {
        let image = Image {
            Created: self.Created,
            Id: self.Id.clone(),
            ParentId: self.ParentId.clone(),
            RepoTags: self.RepoTags.clone(),
            Size: self.Size,
            VirtualSize: self.VirtualSize
        };
        return image;
    }
}
