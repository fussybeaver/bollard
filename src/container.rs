#[derive(RustcEncodable, RustcDecodable)]
#[allow(non_snake_case)]
pub struct Container {
    pub Id: String,
    pub Image: String,
    pub Status: String,
    pub Command: String,
    pub Created: f64,
    pub Names: Vec<String>,
    pub Ports: Vec<String>
}
