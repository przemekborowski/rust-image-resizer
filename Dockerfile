FROM rust:alpine AS builder

RUN apk add --no-cache \
    musl-dev \
    build-base \
    pkgconfig \
    vips-dev \
    clang \
    llvm-dev

ENV LIBCLANG_PATH=/usr/lib
ENV RUSTFLAGS="-C target-feature=-crt-static"

WORKDIR /usr/src/app

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release

FROM alpine:latest

RUN apk add --no-cache vips

WORKDIR /app

COPY --from=builder /usr/src/app/target/release/resizer /usr/local/bin/resizer

COPY public ./public

EXPOSE 3333

CMD ["resizer"]
