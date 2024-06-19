FROM rust:1.79.0-slim

WORKDIR /usr/src/bollard

COPY . .

RUN cargo build
