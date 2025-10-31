FROM rust:1.91 as builder

WORKDIR /app

RUN rustup target add x86_64-unknown-linux-musl

COPY . .

RUN cargo build --release --target x86_64-unknown-linux-musl

FROM alpine:latest

RUN apk add --no-cache ca-certificates

WORKDIR /usr/local/bin

COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/rust_app .

CMD ["./rust_app"]


