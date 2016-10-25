# Boondock: Rust library for talking to the Docker daemon

[![Latest version](https://img.shields.io/crates/v/boondock.svg)](https://crates.io/crates/boondock) [![License](https://img.shields.io/crates/l/boondock.svg)](https://opensource.org/licenses/Apache-2.0) [![Build Status](https://travis-ci.org/faradayio/boondock.svg?branch=master)](https://travis-ci.org/faradayio/boondock) [![Build status](https://ci.appveyor.com/api/projects/status/51vjdqk9p31c5vq9?svg=true)](https://ci.appveyor.com/project/emk/rust-docker) [![Documentation](https://img.shields.io/badge/documentation-docs.rs-yellow.svg)](https://docs.rs/boondock/)

**This is a work in progress!**

This is a fork of Graham Lee's highly useful [rust-docker][] library,
with [hyper][] support from [Toby Lawrence][nuclearfurnace-docker] and
various other recent patches integrated.

It also adds:

- Partial support for Docker 1.12 (ongoing)
- Support for Windows (experimental)
- Support for building without OpenSSL
- Support for finding and connection to the daemon using the same
  `DOCKER_HOST`, `DOCKER_CERT_PATH`, etc. variables as the `docker` command
  line tool
- Consistent error-handling via [error-chain][]

This library is used by the development tool [cage][] to talk to the Docker
daemon.  You're welcome to use it for other things, and we're happy to
accept pull requests!

(Also, the maintainers of [rust-docker][] are totally welcome to use any
code that they like from this fork.  We're mostly maintaining this as a
fork so that we can have very quick turnaround times when we need to fix an
issue with `cage`, and we have no objections to this code being merged back
upstream.)

[rust-docker]: https://brson.github.io/error-chain/error_chain/index.html
[hyper]: http://hyper.rs/
[nuclearfurnace-docker]: https://github.com/nuclearfurnace/rust-docker
[error-chain]: https://brson.github.io/error-chain/error_chain/index.html
[cage]: http://cage.faraday.io/

## Examples

For example code, see the [examples directory](./examples).

## OpenSSL

On the Mac, you can set up OpenSSL as follows:

```bash
brew install openssl
brew link --force openssl

export OPENSSL_INCLUDE_DIR=/usr/local/opt/openssl/include
export OPENSSL_ROOT_DIR=/usr/local/opt/openssl
```

Alternatively, you can build without OpenSSL by passing
`--no-default-features` to `cargo`, or specifying `default-features =
false` in a `Cargo.toml` file.

## Contributing

1. Fork it
2. Create your a new remote upstream repository (`git remote add upstream git@github.com:ghmlee/rust-docker.git`)
3. Commit your changes (`git commit -m 'Add some feature'`)
4. Push to the branch (`git push origin your-branch`)
5. Create new Pull Request
