FROM rust:1.82.0-slim

WORKDIR /usr/src/bollard

COPY . .

RUN cargo build
