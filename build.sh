#!/bin/sh

# apt-get install musl-tools clang llvm -y
export CC_aarch64_unknown_linux_musl=clang
export AR_aarch64_unknown_linux_musl=llvm-ar
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUSTFLAGS="-Clink-self-contained=yes -Clinker=rust-lld"
cargo build --target=aarch64-unknown-linux-musl --release
