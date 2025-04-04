FROM rust:1.86.0-slim

WORKDIR /usr/src/bollard

COPY . .

RUN cargo build
