FROM rust:1.85.0-slim

WORKDIR /usr/src/bollard

COPY . .

RUN cargo build
