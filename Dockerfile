FROM rust:1.80.1-slim

WORKDIR /usr/src/bollard

COPY . .

RUN cargo build
