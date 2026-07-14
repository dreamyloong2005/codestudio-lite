#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAURI_DIR="$ROOT_DIR/src-tauri"
TARGET_TRIPLE="${1:-universal-apple-darwin}"
VERSION="$(/usr/bin/plutil -extract version raw -o - "$TAURI_DIR/tauri.conf.json")"
BUNDLE_ROOT="$TAURI_DIR/target/$TARGET_TRIPLE/release/bundle"
CANONICAL_BASE="CodeStudio-Lite-${VERSION}-macOS-universal"

normalize_file() {
  local source="$1"
  local destination="$2"
  if [[ -f "$source" && "$source" != "$destination" ]]; then
    rm -f "$destination"
    mv "$source" "$destination"
  fi
}

if [[ -d "$BUNDLE_ROOT/macos" ]]; then
  archive="$(find "$BUNDLE_ROOT/macos" -maxdepth 1 -type f -name '*.app.tar.gz' ! -name "${CANONICAL_BASE}.app.tar.gz" -print -quit)"
  if [[ -n "$archive" ]]; then
    normalize_file "$archive" "$BUNDLE_ROOT/macos/${CANONICAL_BASE}.app.tar.gz"
    normalize_file "${archive}.sig" "$BUNDLE_ROOT/macos/${CANONICAL_BASE}.app.tar.gz.sig"
  fi
fi

if [[ -d "$BUNDLE_ROOT/dmg" ]]; then
  dmg="$(find "$BUNDLE_ROOT/dmg" -maxdepth 1 -type f -name '*.dmg' ! -name "${CANONICAL_BASE}.dmg" -print -quit)"
  if [[ -n "$dmg" ]]; then
    normalize_file "$dmg" "$BUNDLE_ROOT/dmg/${CANONICAL_BASE}.dmg"
  fi
fi

echo "macOS release artifacts normalized with base name: ${CANONICAL_BASE}"
