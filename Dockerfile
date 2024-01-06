FROM rust:1.70.0-buster

WORKDIR /usr/src/bollard

COPY . .

RUN cargo build
