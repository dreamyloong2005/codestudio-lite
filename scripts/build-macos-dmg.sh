#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAURI_DIR="$ROOT_DIR/src-tauri"
NODE_BIN="${NODE_BIN:-}"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "macOS DMG packaging must run on macOS." >&2
  exit 1
fi

if [[ -z "$NODE_BIN" ]]; then
  if command -v node >/dev/null 2>&1; then
    NODE_BIN="$(command -v node)"
  elif [[ -x "$HOME/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin/node" ]]; then
    NODE_BIN="$HOME/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin/node"
  else
    echo "Node.js was not found. Set NODE_BIN to a Node executable." >&2
    exit 1
  fi
fi

TAURI_CLI="$ROOT_DIR/node_modules/.bin/tauri"
VITE_CLI="$ROOT_DIR/node_modules/.bin/vite"

if [[ ! -f "$TAURI_CLI" || ! -f "$VITE_CLI" ]]; then
  echo "Missing node_modules CLI dependencies. Run the project dependency install first." >&2
  exit 1
fi

PRODUCT_NAME="$(/usr/bin/plutil -extract productName raw -o - "$TAURI_DIR/tauri.conf.json")"
VERSION="$(/usr/bin/plutil -extract version raw -o - "$TAURI_DIR/tauri.conf.json")"
BUNDLE_IDENTIFIER="$(/usr/bin/plutil -extract identifier raw -o - "$TAURI_DIR/tauri.conf.json")"

TEMP_CONFIG="$(mktemp -t codestudio-tauri-build.XXXXXXXX.json)"
STAGING_DIR="$(mktemp -d -t codestudio-dmg-stage.XXXXXXXX)"
cleanup() {
  rm -f "$TEMP_CONFIG"
  rm -rf "$STAGING_DIR"
}
trap cleanup EXIT

cat >"$TEMP_CONFIG" <<'JSON'
{
  "build": {
    "beforeBuildCommand": ""
  }
}
JSON

echo "Building frontend..."
"$NODE_BIN" "$VITE_CLI" build

echo "Building macOS .app bundle..."
set +e
"$NODE_BIN" "$TAURI_CLI" build --bundles app --ci --no-sign --config "$TEMP_CONFIG" "$@" 2>&1 \
  | awk '
      !/Warn --no-sign flag detected: Signing will be skipped\./ &&
      !/Warn Skipping signing due to --no-sign flag\./
    '
TAURI_BUILD_STATUS=${PIPESTATUS[0]}
set -e
if [[ $TAURI_BUILD_STATUS -ne 0 ]]; then
  exit "$TAURI_BUILD_STATUS"
fi

APP_PATH="$TAURI_DIR/target/release/bundle/macos/${PRODUCT_NAME}.app"
if [[ ! -d "$APP_PATH" ]]; then
  APP_PATH="$(find "$TAURI_DIR/target" -path "*/release/bundle/macos/${PRODUCT_NAME}.app" -type d -print -quit)"
fi
if [[ -z "${APP_PATH:-}" || ! -d "$APP_PATH" ]]; then
  echo "Built macOS app bundle was not found." >&2
  exit 1
fi

MACOS_BUNDLE_IDENTIFIER="$BUNDLE_IDENTIFIER" "$ROOT_DIR/scripts/sign-macos-bundle.sh" "$APP_PATH"

ARCH_LABEL="${DMG_ARCH_LABEL:-}"
if [[ -z "$ARCH_LABEL" ]]; then
  case "$(uname -m)" in
    arm64) ARCH_LABEL="aarch64" ;;
    x86_64) ARCH_LABEL="x64" ;;
    *) ARCH_LABEL="$(uname -m)" ;;
  esac
fi

DMG_DIR="$TAURI_DIR/target/release/bundle/dmg"
DMG_PATH="$DMG_DIR/${PRODUCT_NAME}_${VERSION}_${ARCH_LABEL}.dmg"
mkdir -p "$DMG_DIR"
rm -f "$DMG_PATH"

echo "Staging signed app for DMG..."
/usr/bin/ditto "$APP_PATH" "$STAGING_DIR/${PRODUCT_NAME}.app"
ln -s /Applications "$STAGING_DIR/Applications"

echo "Creating DMG: $DMG_PATH"
/usr/bin/hdiutil create \
  -volname "$PRODUCT_NAME" \
  -srcfolder "$STAGING_DIR" \
  -ov \
  -format UDZO \
  "$DMG_PATH"

/usr/bin/hdiutil verify "$DMG_PATH"
/usr/bin/shasum -a 256 "$DMG_PATH"
echo "DMG built: $DMG_PATH"
