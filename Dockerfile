FROM rust:1.75.0-buster

WORKDIR /usr/src/bollard

COPY . .

RUN cargo build
