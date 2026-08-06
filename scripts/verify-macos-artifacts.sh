#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_TRIPLE="${1:?Usage: scripts/verify-macos-artifacts.sh <target-triple> [version]}"
TAURI_DIR="$ROOT_DIR/src-tauri"
VERSION="${2:-$(/usr/bin/plutil -extract version raw -o - "$TAURI_DIR/tauri.conf.json")}"

case "$TARGET_TRIPLE" in
  aarch64-apple-darwin) ARCH_LABEL="arm64"; EXECUTABLE_ARCH="arm64" ;;
  x86_64-apple-darwin) ARCH_LABEL="x64"; EXECUTABLE_ARCH="x86_64" ;;
  *) echo "Unsupported macOS target: $TARGET_TRIPLE" >&2; exit 1 ;;
esac

BUNDLE_ROOT="$TAURI_DIR/target/$TARGET_TRIPLE/release/bundle"
CANONICAL_BASE="CodeStudio-Lite-${VERSION}-macOS-${ARCH_LABEL}"
APP="$BUNDLE_ROOT/macos/CodeStudio Lite.app"
DMG="$BUNDLE_ROOT/dmg/${CANONICAL_BASE}.dmg"
DMG_SIGNATURE="${DMG}.sig"
ARCHIVE="$BUNDLE_ROOT/macos/${CANONICAL_BASE}.app.tar.gz"
ARCHIVE_SIGNATURE="${ARCHIVE}.sig"
EXECUTABLE="$APP/Contents/MacOS/codestudio-lite"

for required in "$APP" "$DMG" "$DMG_SIGNATURE" "$ARCHIVE" "$ARCHIVE_SIGNATURE"; do
  [[ -e "$required" ]] || { echo "Missing macOS release artifact: $required" >&2; exit 1; }
done
actual_version="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist")"
[[ "$actual_version" == "$VERSION" ]] || {
  echo "Unexpected app version: expected $VERSION, got $actual_version" >&2
  exit 1
}
[[ -s "$DMG_SIGNATURE" ]] || { echo "DMG signature is empty: $DMG_SIGNATURE" >&2; exit 1; }
[[ -s "$ARCHIVE_SIGNATURE" ]] || { echo "Updater archive signature is empty: $ARCHIVE_SIGNATURE" >&2; exit 1; }

actual_architectures="$(lipo -archs "$EXECUTABLE")"
grep -Eq "(^| )${EXECUTABLE_ARCH}( |$)" <<<"$actual_architectures" || {
  echo "Unexpected executable architecture for $TARGET_TRIPLE: $actual_architectures" >&2
  exit 1
}
codesign --verify --deep --strict --verbose=2 "$APP"
hdiutil verify "$DMG"
echo "Verified macOS ${ARCH_LABEL} updater artifacts for ${VERSION}."
