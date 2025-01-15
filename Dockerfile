FROM rust:1.84.0-slim

WORKDIR /usr/src/bollard

COPY . .

RUN cargo build
