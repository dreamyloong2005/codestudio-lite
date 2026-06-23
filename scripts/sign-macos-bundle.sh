#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_PATH="${1:-"$ROOT_DIR/src-tauri/target/release/bundle/macos/CodeStudio Lite.app"}"
BUNDLE_IDENTIFIER="${MACOS_BUNDLE_IDENTIFIER:-com.codestudio.lite}"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "Skipping macOS bundle signing on non-macOS host."
  exit 0
fi

if [[ ! -d "$APP_PATH" ]]; then
  echo "macOS app bundle not found: $APP_PATH" >&2
  exit 1
fi

DESIGNATED_REQUIREMENT="=designated => identifier \"${BUNDLE_IDENTIFIER}\""

echo "Signing $APP_PATH with stable designated requirement: $DESIGNATED_REQUIREMENT"
/usr/bin/codesign \
  --force \
  --deep \
  --options runtime \
  --sign - \
  --requirements "$DESIGNATED_REQUIREMENT" \
  "$APP_PATH"

DR_OUTPUT="$(/usr/bin/codesign -dr - "$APP_PATH" 2>&1)"
echo "$DR_OUTPUT"

if ! grep -Fq "designated => identifier \"${BUNDLE_IDENTIFIER}\"" <<<"$DR_OUTPUT"; then
  echo "macOS app bundle did not keep the stable designated requirement." >&2
  exit 1
fi

/usr/bin/codesign -v --deep --strict --verbose=4 "$APP_PATH"
echo "Stable macOS bundle signature verified."
