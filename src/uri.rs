use hyper::Uri as HyperUri;
use log::trace;
use percent_encoding::{
    percent_decode_str, utf8_percent_encode, AsciiSet, PercentEncode, NON_ALPHANUMERIC,
};
use url::Url;

use std::borrow::Cow;
use std::ffi::OsStr;

use crate::docker::{ClientType, ClientVersion};
use crate::errors::Error;

/// Bytes escaped in an identifier that is interpolated into a request path:
/// everything except RFC 3986 unreserved characters, `:` and `@`.
const PATH_SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~')
    .remove(b':')
    .remove(b'@');

/// Like [`PATH_SEGMENT`], but keeps `/` so that image and plugin references
/// such as `library/ubuntu:latest` are sent unchanged.
const REFERENCE: &AsciiSet = &PATH_SEGMENT.remove(b'/');

/// Escapes an identifier (container, network, volume, ...) for use as a
/// single request path segment, so that it cannot introduce `/`, `?`, `#`
/// or `%` into the request path.
pub(crate) fn escape_segment(value: &str) -> PercentEncode<'_> {
    utf8_percent_encode(value, PATH_SEGMENT)
}

/// Escapes an image or plugin reference for use in a request path. `/` is
/// kept, since the daemon routes these as `{name:.*}`; `.` and `..` segments
/// are rejected by [`Uri::parse`].
pub(crate) fn escape_reference(value: &str) -> PercentEncode<'_> {
    utf8_percent_encode(value, REFERENCE)
}

/// Rejects paths containing `.` or `..` segments once percent-decoded, as
/// both `url` and the daemon would resolve them and let an identifier escape
/// its position in the path.
pub(crate) fn check_dot_segments(path: &str) -> Result<(), Error> {
    let decoded: Cow<'_, [u8]> = percent_decode_str(path).into();
    if decoded
        .split(|&b| b == b'/')
        .any(|segment| segment == b"." || segment == b"..")
    {
        return Err(Error::InvalidRequestPathError {
            path: path.to_owned(),
        });
    }
    Ok(())
}

#[derive(Debug)]
pub struct Uri<'a> {
    encoded: Cow<'a, str>,
}

impl<'a> TryFrom<Uri<'a>> for HyperUri {
    type Error = http::uri::InvalidUri;

    fn try_from(uri: Uri<'a>) -> Result<Self, Self::Error> {
        uri.encoded.as_ref().parse()
    }
}

impl<'a> Uri<'a> {
    pub(crate) fn parse<O>(
        socket: &'a str,
        client_type: &ClientType,
        path: &'a str,
        query: Option<O>,
        client_version: Option<&ClientVersion>,
    ) -> Result<Self, Error>
    where
        O: serde::ser::Serialize,
    {
        check_dot_segments(path)?;
        let mut url = Url::parse(&format!(
            "{}://{}",
            Uri::socket_scheme(client_type),
            Uri::socket_host(socket, client_type)
        ))?;
        match client_version {
            Some(version) => url.set_path(&format!(
                "/v{}.{}{}",
                version.major_version, version.minor_version, path
            )),
            None => url.set_path(path),
        }

        if let Some(pairs) = query {
            trace!("pairs: {}", serde_json::to_string(&pairs)?);

            let qs = serde_urlencoded::to_string(pairs)?;
            url.set_query(Some(&qs));
        }

        trace!(
            "Parsing uri: {}, client_type: {:?}, socket: {}",
            url.as_str(),
            client_type,
            socket
        );
        Ok(Uri {
            encoded: Cow::Owned(url.as_str().to_owned()),
        })
    }

    fn socket_host<P>(socket: P, client_type: &ClientType) -> String
    where
        P: AsRef<OsStr>,
    {
        match client_type {
            #[cfg(feature = "http")]
            ClientType::Http => socket.as_ref().to_string_lossy().into_owned(),
            #[cfg(feature = "ssl_providerless")]
            ClientType::SSL => socket.as_ref().to_string_lossy().into_owned(),
            #[cfg(all(feature = "pipe", unix))]
            ClientType::Unix => hex::encode(socket.as_ref().to_string_lossy().as_bytes()),
            #[cfg(all(feature = "pipe", windows))]
            ClientType::NamedPipe => hex::encode(socket.as_ref().to_string_lossy().as_bytes()),
            #[cfg(feature = "ssh")]
            ClientType::Ssh => socket.as_ref().to_string_lossy().into_owned(),
            ClientType::Custom { .. } => socket.as_ref().to_string_lossy().into_owned(),
        }
    }

    fn socket_scheme(client_type: &'a ClientType) -> &'a str {
        match client_type {
            #[cfg(feature = "http")]
            ClientType::Http => "http",
            #[cfg(feature = "ssl_providerless")]
            ClientType::SSL => "https",
            #[cfg(all(feature = "pipe", unix))]
            ClientType::Unix => "unix",
            #[cfg(all(feature = "pipe", windows))]
            ClientType::NamedPipe => "net.pipe",
            #[cfg(feature = "ssh")]
            ClientType::Ssh => "ssh",
            ClientType::Custom { scheme } => scheme.as_str(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VERSION: ClientVersion = ClientVersion {
        major_version: 1,
        minor_version: 53,
    };

    fn client_type() -> ClientType {
        ClientType::Custom {
            scheme: String::from("http"),
        }
    }

    #[test]
    fn parse_retains_api_version_prefix() {
        let uri = Uri::parse(
            "localhost:2375",
            &client_type(),
            "/containers/json",
            None::<String>,
            Some(&VERSION),
        )
        .unwrap();
        let uri: HyperUri = uri.try_into().unwrap();

        assert_eq!(uri.path(), "/v1.53/containers/json");
    }

    #[test]
    fn parse_appends_query_after_version_prefix() {
        let query = std::collections::HashMap::from([("all", "true")]);
        let uri = Uri::parse(
            "localhost:2375",
            &client_type(),
            "/containers/json",
            Some(query),
            Some(&VERSION),
        )
        .unwrap();
        let uri: HyperUri = uri.try_into().unwrap();

        assert_eq!(
            uri.path_and_query().unwrap().as_str(),
            "/v1.53/containers/json?all=true"
        );
    }

    #[test]
    fn parse_percent_encodes_path_segments() {
        let uri = Uri::parse(
            "localhost:2375",
            &client_type(),
            "/containers/my container/json",
            None::<String>,
            Some(&VERSION),
        )
        .unwrap();
        let uri: HyperUri = uri.try_into().unwrap();

        assert_eq!(uri.path(), "/v1.53/containers/my%20container/json");
    }

    #[test]
    fn parse_confines_delimiters_to_the_path() {
        let uri = Uri::parse(
            "localhost:2375",
            &client_type(),
            "/containers/a?b#c/json",
            None::<String>,
            Some(&VERSION),
        )
        .unwrap();
        let uri: HyperUri = uri.try_into().unwrap();

        assert_eq!(
            uri.path_and_query().unwrap().as_str(),
            "/v1.53/containers/a%3Fb%23c/json"
        );
    }

    fn parse_path(path: &str) -> Result<String, Error> {
        let uri: HyperUri = Uri::parse(
            "localhost:2375",
            &client_type(),
            path,
            Some([("follow", "true")]),
            Some(&VERSION),
        )?
        .try_into()
        .unwrap();
        Ok(uri.path_and_query().unwrap().as_str().to_owned())
    }

    #[test]
    fn escaped_segment_stays_in_place() {
        let cases = [
            ("my-container_1.2", "my-container_1.2"),
            ("foo/json?", "foo%2Fjson%3F"),
            ("a#b", "a%23b"),
            ("%2e%2e", "%252e%252e"),
        ];
        for (name, escaped) in cases {
            let path = format!("/containers/{}/logs", escape_segment(name));
            assert_eq!(
                parse_path(&path).unwrap(),
                format!("/v1.53/containers/{escaped}/logs?follow=true"),
                "name: {name}"
            );
        }
    }

    #[test]
    fn escaped_reference_keeps_slashes() {
        let path = format!(
            "/images/{}/json",
            escape_reference("registry:5000/library/ubuntu:latest")
        );
        assert_eq!(
            parse_path(&path).unwrap(),
            "/v1.53/images/registry:5000/library/ubuntu:latest/json?follow=true"
        );

        let path = format!("/images/{}/json", escape_reference("a?b#c"));
        assert_eq!(
            parse_path(&path).unwrap(),
            "/v1.53/images/a%3Fb%23c/json?follow=true"
        );
    }

    #[test]
    fn parse_rejects_dot_segments() {
        for path in [
            format!("/containers/{}/logs", escape_segment("..")),
            format!("/containers/{}/logs", escape_segment(".")),
            format!("/images/{}", escape_reference("x/../../containers/y")),
            format!("/networks/{}", escape_segment("x/../images/y")),
            format!(
                "/containers/{}/logs",
                escape_segment("../../containers/json?")
            ),
            "/containers/%2E%2e/logs".to_owned(),
            "/containers/.%2E/logs".to_owned(),
        ] {
            assert!(
                matches!(
                    parse_path(&path),
                    Err(Error::InvalidRequestPathError { .. })
                ),
                "path: {path}"
            );
        }
    }

    #[test]
    fn parse_without_version_omits_prefix() {
        let uri = Uri::parse(
            "localhost:2375",
            &client_type(),
            "/version",
            None::<String>,
            None,
        )
        .unwrap();
        let uri: HyperUri = uri.try_into().unwrap();

        assert_eq!(uri.path(), "/version");
    }
}
