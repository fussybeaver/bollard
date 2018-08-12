FROM ekidd/rust-musl-builder AS builder

WORKDIR /tmp/boondock

COPY . ./

RUN sudo chown -R rust:rust /tmp/boondock
RUN cargo build --example version_tls --example version_http --example version_unix

FROM alpine:3.7
RUN apk --no-cache add ca-certificates

COPY --from=builder /tmp/boondock/target/x86_64-unknown-linux-musl/debug/examples/* /usr/local/bin/
