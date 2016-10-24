//! Error-handling with the `error_chain` crate.

use std::env;
use std::io;

error_chain! {
    foreign_links {
        io::Error, Io;
        env::VarError, EnvVar;
    }

    errors {
        CouldNotConnect(host: String) {
            description("could not connect to Docker")
            display("could not connected to Docker at '{}'", &host)
        }

        NoCertPath {
            description("could not find DOCKER_CERT_PATH")
            display("could not find DOCKER_CERT_PATH")
        }

        UnsupportedScheme(host: String) {
            description("unsupported Docker URL scheme")
            display("do not know how to connect to Docker at '{}'", &host)
        }
    }
}
