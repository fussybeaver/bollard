FROM ekidd/rust-musl-builder AS builder

WORKDIR /tmp/boondock

COPY . ./

RUN sudo chown -R rust:rust /tmp/boondock \
  && sudo groupadd --gid 999 docker \
  && sudo usermod -a -G docker rust

RUN cargo build
