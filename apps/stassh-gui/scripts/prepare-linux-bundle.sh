#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
app_dir="$(cd "$script_dir/.." && pwd)"
workspace_dir="$(cd "$app_dir/../.." && pwd)"
target_triple="${STASSH_PACKAGE_TARGET:-x86_64-unknown-linux-gnu}"

if [[ "$target_triple" != "x86_64-unknown-linux-gnu" ]]; then
  echo "unsupported STASSH_PACKAGE_TARGET: $target_triple" >&2
  echo "only x86_64-unknown-linux-gnu packaging is configured" >&2
  exit 1
fi

cargo build \
  --manifest-path "$workspace_dir/Cargo.toml" \
  --release \
  --target "$target_triple" \
  --package stassh \
  --package stassh-tui

binary_dir="$app_dir/src-tauri/binaries"
mkdir -p "$binary_dir"

install -m 755 \
  "$workspace_dir/target/$target_triple/release/stassh" \
  "$binary_dir/stassh-$target_triple"
install -m 755 \
  "$workspace_dir/target/$target_triple/release/stassh-tui" \
  "$binary_dir/stassh-tui-$target_triple"
