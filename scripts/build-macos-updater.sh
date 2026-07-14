#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "macOS updater packaging must run on macOS." >&2
  exit 1
fi

VERSION="$(/usr/bin/plutil -extract version raw -o - "$ROOT_DIR/src-tauri/tauri.conf.json")"

for target_and_arch in \
  "aarch64-apple-darwin:arm64" \
  "x86_64-apple-darwin:x64"; do
  target="${target_and_arch%%:*}"
  arch="${target_and_arch##*:}"

  npm run tauri:build:updater -- --target "$target" "$@"
  bash "$ROOT_DIR/scripts/normalize-macos-artifacts.sh" "$target"

  dmg_path="$ROOT_DIR/src-tauri/target/$target/release/bundle/dmg/CodeStudio-Lite-${VERSION}-macOS-${arch}.dmg"
  if [[ ! -f "$dmg_path" ]]; then
    echo "Normalized macOS updater DMG was not found: $dmg_path" >&2
    exit 1
  fi
  npx tauri signer sign "$dmg_path"
  if [[ ! -f "$dmg_path.sig" ]]; then
    echo "macOS updater signature was not generated: $dmg_path.sig" >&2
    exit 1
  fi
done
