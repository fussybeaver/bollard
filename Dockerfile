FROM rust:1.99.0-slim

# for `ssh` feature
RUN apt-get update && apt-get install --yes openssh-client

WORKDIR /usr/src/bollard

COPY . .

RUN cargo build
