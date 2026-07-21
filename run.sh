#!/usr/bin/env bash
# kiraterm launcher — uses nix-shell to resolve Wayland/Vulkan/XKB paths on NixOS.
set -e
cd "$(dirname "$0")"

if command -v nix-shell >/dev/null 2>&1; then
  exec nix-shell shell.nix --run '
    set -e
    if [ ! -x target/release/kiraterm ]; then
      echo "Building kiraterm (first build, may take a few minutes)..."
      cargo build --release
    fi
    exec ./target/release/kiraterm
  '
else
  # Non-Nix fallback: assume system already has the graphics libs on the loader path.
  if [ ! -x target/release/kiraterm ]; then
    echo "Building kiraterm..."
    cargo build --release
  fi
  exec ./target/release/kiraterm
fi
