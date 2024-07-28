FROM rust:1.80.0-slim

WORKDIR /usr/src/bollard

COPY . .

RUN cargo build
