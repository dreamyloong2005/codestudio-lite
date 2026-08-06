#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_TRIPLE="${1:?Usage: scripts/normalize-macos-artifacts.sh <target-triple>}"
TAURI_DIR="$ROOT_DIR/src-tauri"
VERSION="$(/usr/bin/plutil -extract version raw -o - "$TAURI_DIR/tauri.conf.json")"
BUNDLE_ROOT="$TAURI_DIR/target/$TARGET_TRIPLE/release/bundle"
node "$ROOT_DIR/scripts/normalize-macos-artifacts.mjs" "$TARGET_TRIPLE" "$VERSION" "$BUNDLE_ROOT"
