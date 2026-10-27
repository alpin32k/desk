#!/bin/sh
# Builds the tmux that ships inside desk (macOS app, AppImage) for people who
# have none: libevent and utf8proc linked in statically, ncurses and libc from
# the system. Writes src-tauri/binaries/tmux-<target triple>, where Tauri's
# externalBin looks for it (see src-tauri/tauri.tmux.conf.json).
# Usage: sh .github/build-tmux.sh <target triple>, e.g. aarch64-apple-darwin
set -eu
target="$1"
root=$(cd "$(dirname "$0")/.." && pwd)
out="$root/src-tauri/binaries/tmux-$target"
work="${TMPDIR:-/tmp}/desk-tmux-$$"
prefix="$work/prefix"
mkdir -p "$work" "$prefix/include" "$prefix/lib" "$(dirname "$out")"
trap 'rm -rf "$work"' EXIT
cd "$work"

fetch() { # url sha256
  file=$(basename "$1")
  curl -fsSL -o "$file" "$1"
  echo "$2  $file" | shasum -a 256 -c - >/dev/null
  tar xzf "$file"
}
fetch https://github.com/libevent/libevent/releases/download/release-2.1.12-stable/libevent-2.1.12-stable.tar.gz \
  92e6de1be9ec176428fd2367677e61ceffc2ee1cb119035037a27d346b0403bb
fetch https://github.com/JuliaStrings/utf8proc/releases/download/v2.10.0/utf8proc-2.10.0.tar.gz \
  276a37dc4d1dd24d7896826a579f4439d1e5fe33603add786bb083cab802e23e
fetch https://github.com/tmux/tmux/releases/download/3.6b/tmux-3.6b.tar.gz \
  390759d25fdba016887ec982b808927e637070fd7d03a8021f8ef3102b9ae3c7

jobs=$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 2)
export MACOSX_DEPLOYMENT_TARGET=11.0

(cd libevent-2.1.12-stable &&
  ./configure --prefix="$prefix" --disable-shared --enable-static --disable-openssl \
    --disable-samples --disable-libevent-regress --disable-debug-mode >/dev/null &&
  make -j"$jobs" >/dev/null && make install >/dev/null)

(cd utf8proc-2.10.0 && make -j"$jobs" libutf8proc.a >/dev/null &&
  cp utf8proc.h "$prefix/include/" && cp libutf8proc.a "$prefix/lib/")

(cd tmux-3.6b &&
  ./configure --enable-utf8proc \
    LIBEVENT_CORE_CFLAGS="-I$prefix/include" LIBEVENT_CORE_LIBS="$prefix/lib/libevent_core.a" \
    LIBEVENT_CFLAGS="-I$prefix/include" LIBEVENT_LIBS="$prefix/lib/libevent_core.a" \
    LIBUTF8PROC_CFLAGS="-I$prefix/include" LIBUTF8PROC_LIBS="$prefix/lib/libutf8proc.a" \
    CPPFLAGS="-I$prefix/include" >/dev/null &&
  make -j"$jobs" >/dev/null)

cp tmux-3.6b/tmux "$out"
strip "$out"
# Apple Silicon runs only signed code; strip drops the linker's ad-hoc signature.
if [ "$(uname)" = Darwin ]; then codesign -s - -f "$out"; fi
"$out" -V
