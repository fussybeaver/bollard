MAKEFILE := $(abspath $(lastword $(MAKEFILE_LIST)))
PROJECT := $(dir $(MAKEFILE))
CARGO := $(PROJECT)/Cargo.toml

export CARGO
export OPENSSL_INCLUDE_DIR=/usr/local/opt/openssl/include
export OPENSSL_ROOT_DIR=/usr/local/opt/openssl

default: build

build:
	@cargo build --manifest-path $(CARGO)

run:
	cargo run

clean:
	cargo clean
