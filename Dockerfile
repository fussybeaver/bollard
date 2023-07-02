FROM rust:1.67.0-buster

WORKDIR /usr/src/bollard

COPY . .

RUN cargo build
