FROM rust:1.65.0-buster

WORKDIR /usr/src/bollard

COPY . .

RUN cargo build
