FROM rust:1.84.1-slim

WORKDIR /usr/src/bollard

COPY . .

RUN cargo build
