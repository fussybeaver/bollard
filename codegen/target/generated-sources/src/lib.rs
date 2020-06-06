#![allow(missing_docs, trivial_casts, unused_variables, unused_mut, unused_imports, unused_extern_crates, non_camel_case_types)]
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;

use std::io::Error;

#[allow(unused_imports)]
use std::collections::HashMap;

pub const BASE_PATH: &'static str = "/v1.40";

pub mod models;
