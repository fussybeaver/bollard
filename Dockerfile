FROM rust:1.81.0-slim

WORKDIR /usr/src/bollard

COPY . .

RUN cargo build
