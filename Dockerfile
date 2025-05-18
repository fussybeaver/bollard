FROM rust:1.87.0-slim

WORKDIR /usr/src/bollard

COPY . .

RUN cargo build
