#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "macOS updater packaging must run on macOS." >&2
  exit 1
fi

npm run tauri:build:updater -- --target universal-apple-darwin "$@"
bash "$ROOT_DIR/scripts/normalize-macos-artifacts.sh" universal-apple-darwin

DMG_PATH="$ROOT_DIR/src-tauri/target/universal-apple-darwin/release/bundle/dmg/CodeStudio-Lite-$(/usr/bin/plutil -extract version raw -o - "$ROOT_DIR/src-tauri/tauri.conf.json")-macOS-universal.dmg"
if [[ ! -f "$DMG_PATH" ]]; then
  echo "Normalized macOS updater DMG was not found: $DMG_PATH" >&2
  exit 1
fi
npx tauri signer sign "$DMG_PATH"
if [[ ! -f "$DMG_PATH.sig" ]]; then
  echo "macOS updater signature was not generated: $DMG_PATH.sig" >&2
  exit 1
fi
