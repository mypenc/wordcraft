#!/usr/bin/env bash
# Fetch winit 0.30.13 (the version Cargo.lock pins) into android/vendor/winit and apply the
# Android input patch: mouse hover/buttons/wheel, keyboard modifiers and soft-keyboard text.
# The stock Android backend drops all of those (`_ => None // TODO mouse events`).
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
dest="$here/vendor/winit"
version="0.30.13"
if [ -e "$dest" ]; then echo "$dest exists; remove it to re-vendor"; exit 0; fi
mkdir -p "$here/vendor"
tmp="$(mktemp -d)"
curl -fsSL "https://static.crates.io/crates/winit/winit-$version.crate" -o "$tmp/winit.crate"
tar -xzf "$tmp/winit.crate" -C "$tmp"
mv "$tmp/winit-$version" "$dest"
patch -p1 -d "$dest" < "$here/patches/winit-$version-android-input.patch"
rm -rf "$tmp"
echo "winit $version patched in $dest"
