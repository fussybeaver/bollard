FROM rust:1.61.0-buster

WORKDIR /usr/src/bollard

COPY . .

RUN cargo build
