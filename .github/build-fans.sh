#!/bin/sh
# Builds desk-fans, the root helper that turns the fans to full speed when the
# CPU is hot (src-tauri/fans-helper, src-tauri/src/fans.rs), for the macOS app.
# Writes src-tauri/binaries/desk-fans-<target triple>, where Tauri's
# externalBin looks for it (see src-tauri/tauri.mac-bundle.conf.json).
# Usage: sh .github/build-fans.sh <target triple>, e.g. aarch64-apple-darwin
set -eu
target="$1"
root=$(cd "$(dirname "$0")/.." && pwd)
cargo build --release --manifest-path "$root/src-tauri/fans-helper/Cargo.toml" --target "$target"
mkdir -p "$root/src-tauri/binaries"
cp "$root/src-tauri/fans-helper/target/$target/release/desk-fans" "$root/src-tauri/binaries/desk-fans-$target"
