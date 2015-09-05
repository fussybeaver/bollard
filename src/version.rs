#[derive(RustcEncodable, RustcDecodable)]
#[allow(non_snake_case)]
pub struct Version {
   pub Version: String,
   pub ApiVersion: String,
   pub GitCommit: String,
   pub GoVersion: String,
   pub Os: String,
   pub Arch: String,
   pub KernelVersion: String,
   pub BuildTime: Option<String>,
   pub Experimental:Option<bool>
}
