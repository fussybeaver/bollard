//! Regression test for GHSA-5qvq-cm5w-v6qq: `connect_with_ssl` must treat the
//! caller's `ca.pem` as the only trust anchor for the daemon.
//!
//! Runs without a Docker daemon or network. A TLS server on 127.0.0.1 answers
//! `/_ping`, and `SSL_CERT_FILE` points rustls-native-certs at a throwaway
//! "platform" root, so a certificate issued by that root stands in for one
//! issued by any public or enterprise CA in the OS store.
//!
//! Before the fix, `webpki` / `test_ssl` builds used bundled Mozilla roots
//! instead of the OS store. This reproduction injects an OS root, so it only
//! distinguishes vulnerable from fixed behavior with the native-roots providers:
//!
//! ```text
//! cargo test --features ssl --test ssl_trust_anchor_test
//! cargo test --features aws-lc-rs --test ssl_trust_anchor_test
//! ```
//!
//! `platform_root_signed_daemon_is_rejected` fails until the root store is
//! built from `ca.pem` alone; the other two tests are controls.
#![cfg(all(
    any(feature = "ssl", feature = "aws-lc-rs"),
    not(any(feature = "test_ssl", feature = "webpki"))
))]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::thread;

use bollard::{Docker, API_DEFAULT_VERSION};
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, CertifiedIssuer, DnType, IsCa, KeyPair,
    KeyUsagePurpose,
};
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};

struct Ca {
    issuer: CertifiedIssuer<'static, KeyPair>,
}

impl Ca {
    fn new(name: &str) -> Ca {
        let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
        params.distinguished_name.push(DnType::CommonName, name);
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        let issuer = CertifiedIssuer::self_signed(params, KeyPair::generate().unwrap()).unwrap();
        Ca { issuer }
    }

    fn pem(&self) -> String {
        self.issuer.pem()
    }

    /// Issue a leaf for `san` (an IP address or DNS name).
    fn issue(&self, san: &str) -> (Certificate, KeyPair) {
        let key = KeyPair::generate().unwrap();
        let params = CertificateParams::new(vec![san.to_string()]).unwrap();
        (params.signed_by(&key, &self.issuer).unwrap(), key)
    }
}

struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    /// Writes `ca.pem` plus a client `cert.pem` / `key.pem` signed by it.
    fn new(docker_ca: &Ca) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let (client_cert, client_key) = docker_ca.issue("client");
        std::fs::write(dir.path().join("ca.pem"), docker_ca.pem()).unwrap();
        std::fs::write(dir.path().join("cert.pem"), client_cert.pem()).unwrap();
        std::fs::write(dir.path().join("key.pem"), client_key.serialize_pem()).unwrap();
        Fixture { dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
}

/// Point rustls-native-certs at a single throwaway root for the whole binary.
/// `OnceLock` keeps `set_var` from racing a concurrent `load_native_certs`.
fn platform_root() -> &'static Ca {
    static ROOT: OnceLock<Ca> = OnceLock::new();
    ROOT.get_or_init(|| {
        let root = Ca::new("Stand-in platform root");
        let path = std::env::temp_dir().join(format!(
            "bollard-ghsa-5qvq-platform-root-{}.pem",
            std::process::id()
        ));
        std::fs::write(&path, root.pem()).unwrap();
        std::env::set_var("SSL_CERT_FILE", &path);
        std::env::remove_var("SSL_CERT_DIR");
        root
    })
}

/// Serve one HTTPS connection on 127.0.0.1 that answers any request with `OK`.
fn serve_once(cert: &Certificate, key: &KeyPair) -> u16 {
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der()));
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert.der().clone()], key)
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        let (tcp, _) = listener.accept().unwrap();
        let conn = rustls::ServerConnection::new(Arc::new(config)).unwrap();
        let mut tls = rustls::StreamOwned::new(conn, tcp);
        let mut request = Vec::new();
        let mut buf = [0u8; 1024];
        // A rejected handshake surfaces here as an error; that is expected.
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            match tls.read(&mut buf) {
                Ok(0) | Err(_) => return,
                Ok(n) => request.extend_from_slice(&buf[..n]),
            }
        }
        let _ = tls.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK",
        );
        let _ = tls.flush();
    });
    port
}

async fn ping(port: u16, fixture: &Fixture) -> Result<String, bollard::errors::Error> {
    let docker = Docker::connect_with_ssl(
        &format!("https://127.0.0.1:{port}"),
        &fixture.path("key.pem"),
        &fixture.path("cert.pem"),
        &fixture.path("ca.pem"),
        10,
        API_DEFAULT_VERSION,
    )?;
    docker.ping().await
}

fn is_unknown_issuer(err: &bollard::errors::Error) -> bool {
    format!("{err:?}").contains("UnknownIssuer")
}

#[tokio::test]
async fn ssl_ca_signed_daemon_is_accepted() {
    platform_root();
    let docker_ca = Ca::new("Docker CA");
    let fixture = Fixture::new(&docker_ca);
    let (cert, key) = docker_ca.issue("127.0.0.1");

    let result = ping(serve_once(&cert, &key), &fixture).await;
    assert_eq!(result.unwrap(), "OK");
}

#[tokio::test]
async fn unrelated_issuer_is_rejected() {
    platform_root();
    let docker_ca = Ca::new("Docker CA");
    let fixture = Fixture::new(&docker_ca);
    let (cert, key) = Ca::new("Unrelated CA").issue("127.0.0.1");

    let err = ping(serve_once(&cert, &key), &fixture).await.unwrap_err();
    assert!(is_unknown_issuer(&err), "unexpected error: {err:?}");
}

/// GHSA-5qvq-cm5w-v6qq: a daemon certificate from a platform root must not be
/// trusted when the caller supplied its own `ca.pem`.
#[tokio::test]
async fn platform_root_signed_daemon_is_rejected() {
    let platform = platform_root();
    let docker_ca = Ca::new("Docker CA");
    let fixture = Fixture::new(&docker_ca);
    let (cert, key) = platform.issue("127.0.0.1");

    match ping(serve_once(&cert, &key), &fixture).await {
        Ok(body) => panic!(
            "handshake succeeded with a certificate outside ca.pem (server answered {body:?})"
        ),
        Err(err) => assert!(is_unknown_issuer(&err), "unexpected error: {err:?}"),
    }
}
