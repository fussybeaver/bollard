FROM rust:1.83.0-slim

WORKDIR /usr/src/bollard

COPY . .

RUN cargo build
