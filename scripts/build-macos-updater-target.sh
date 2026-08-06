#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TARGET_TRIPLE="${1:?Usage: scripts/build-macos-updater-target.sh <target-triple> [--frontend-ready] [tauri args...]}"
shift
FRONTEND_READY=false
remaining_args=()
for argument in "$@"; do
  if [[ "$argument" == "--frontend-ready" ]]; then
    FRONTEND_READY=true
  else
    remaining_args+=("$argument")
  fi
done

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "macOS updater packaging must run on macOS." >&2
  exit 1
fi
if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" || -z "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" ]]; then
  echo "TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD are required." >&2
  exit 1
fi

case "$TARGET_TRIPLE" in
  aarch64-apple-darwin) TAURI_ARCH="aarch64" ;;
  x86_64-apple-darwin) TAURI_ARCH="x64" ;;
  *) echo "Unsupported macOS target: $TARGET_TRIPLE" >&2; exit 1 ;;
esac

TAURI_DIR="$ROOT_DIR/src-tauri"
VERSION="$(/usr/bin/plutil -extract version raw -o - "$TAURI_DIR/tauri.conf.json")"
BUNDLE_ROOT="$TAURI_DIR/target/$TARGET_TRIPLE/release/bundle"
CANONICAL_ARCH="$([[ "$TARGET_TRIPLE" == "aarch64-apple-darwin" ]] && echo arm64 || echo x64)"
CANONICAL_BASE="CodeStudio-Lite-${VERSION}-macOS-${CANONICAL_ARCH}"
RAW_DMG="$BUNDLE_ROOT/dmg/CodeStudio Lite_${VERSION}_${TAURI_ARCH}.dmg"
RAW_ARCHIVE="$BUNDLE_ROOT/macos/CodeStudio Lite.app.tar.gz"
RAW_ARCHIVE_SIGNATURE="${RAW_ARCHIVE}.sig"
CANONICAL_DMG="$BUNDLE_ROOT/dmg/${CANONICAL_BASE}.dmg"
CANONICAL_ARCHIVE="$BUNDLE_ROOT/macos/${CANONICAL_BASE}.app.tar.gz"
CANONICAL_ARCHIVE_SIGNATURE="${CANONICAL_ARCHIVE}.sig"
GENERATED_CONFIG="$TAURI_DIR/tauri.updater.generated.conf.json"

cleanup() {
  rm -f "$GENERATED_CONFIG"
}
trap cleanup EXIT

if [[ "$FRONTEND_READY" != true ]]; then
  npm run build
fi

rm -f "$RAW_DMG" "$RAW_ARCHIVE" "$RAW_ARCHIVE_SIGNATURE" "$CANONICAL_DMG" "$CANONICAL_ARCHIVE" "$CANONICAL_ARCHIVE_SIGNATURE"
node scripts/prepare-updater-config.mjs --skip-before-build
if (( ${#remaining_args[@]} > 0 )); then
  npx tauri build --config "$GENERATED_CONFIG" --target "$TARGET_TRIPLE" "${remaining_args[@]}"
else
  npx tauri build --config "$GENERATED_CONFIG" --target "$TARGET_TRIPLE"
fi
node scripts/normalize-macos-artifacts.mjs "$TARGET_TRIPLE" "$VERSION" "$BUNDLE_ROOT"
npx tauri signer sign "$CANONICAL_DMG"
bash scripts/verify-macos-artifacts.sh "$TARGET_TRIPLE" "$VERSION"
