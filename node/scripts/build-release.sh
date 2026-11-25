#!/usr/bin/env bash
# Reproducible release build. Same commit + same platform + the toolchain pinned in
# rust-toolchain.toml should give the same SHA-256, wherever the source is checked out.
set -euo pipefail
cd "$(dirname "$0")/.."
SRC=$(pwd)
CARGO_HOME=${CARGO_HOME:-$HOME/.cargo}
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$SRC/target}
# Strip machine-specific paths from the binary (panic messages, file names).
export RUSTFLAGS="--remap-path-prefix=$SRC=/synterra-node --remap-path-prefix=$CARGO_HOME=/cargo"
cargo build --release --locked
sha256sum "$CARGO_TARGET_DIR/release/synterra"
