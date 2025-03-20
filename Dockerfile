FROM rust:1.85.1-slim

WORKDIR /usr/src/bollard

COPY . .

RUN cargo build
