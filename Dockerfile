# Builds a fully static Linux x86_64 binary using Alpine (musl libc).
# The resulting binary runs on any Linux distribution without glibc dependencies.

FROM rust:alpine AS builder

RUN apk add --no-cache \
    musl-dev \
    pkgconf \
    openssl-dev \
    openssl-libs-static \
    libssh2-dev \
    libssh2-static \
    zlib-dev \
    zlib-static

WORKDIR /build
COPY Cargo.toml Cargo.lock* ./

# Cache dependencies
RUN mkdir src && printf 'fn main(){}' > src/main.rs
RUN PKG_CONFIG_ALL_STATIC=1 cargo fetch
RUN PKG_CONFIG_ALL_STATIC=1 OPENSSL_STATIC=1 cargo build --release 2>/dev/null || true
RUN rm -rf src

COPY src ./src
RUN touch src/main.rs

RUN PKG_CONFIG_ALL_STATIC=1 OPENSSL_STATIC=1 cargo build --release

RUN mkdir /dist && cp target/release/filesync /dist/filesync
