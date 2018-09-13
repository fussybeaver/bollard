use failure::Error;

pub trait EncodableQueryString {
    fn into_array<'a>(self) -> Result<Vec<(&'a str, String)>, Error>;
}

#[derive(Debug, Clone, Serialize)]
pub struct NoParams {}

impl EncodableQueryString for NoParams {
    fn into_array<'a>(self) -> Result<Vec<(&'a str, String)>, Error> {
        unreachable!()
    }
}
