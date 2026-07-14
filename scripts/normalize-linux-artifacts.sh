#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAURI_DIR="$ROOT_DIR/src-tauri"
TARGET_TRIPLE="${1:-${CARGO_BUILD_TARGET:-}}"
if command -v node >/dev/null 2>&1; then
  VERSION="$(cd "$ROOT_DIR" && node -p "require('./package.json').version")"
elif command -v python3 >/dev/null 2>&1; then
  VERSION="$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["version"])' "$ROOT_DIR/package.json")"
else
  echo "Node.js or Python 3 is required to read the package version." >&2
  exit 1
fi

architecture_source="${TARGET_TRIPLE:-$(uname -m)}"
case "$architecture_source" in
  x86_64*|amd64*) ARCH_LABEL="x64" ;;
  aarch64*|arm64*) ARCH_LABEL="aarch64" ;;
  armv7*) ARCH_LABEL="armv7" ;;
  i686*|i386*) ARCH_LABEL="x86" ;;
  *) ARCH_LABEL="${architecture_source%%-*}" ;;
esac

if [[ -n "$TARGET_TRIPLE" ]]; then
  BUNDLE_ROOT="$TAURI_DIR/target/$TARGET_TRIPLE/release/bundle"
else
  BUNDLE_ROOT="$TAURI_DIR/target/release/bundle"
fi
CANONICAL_BASE="CodeStudio-Lite-${VERSION}-Linux-${ARCH_LABEL}"

normalize_artifact() {
  local directory="$1"
  local pattern="$2"
  local suffix="$3"
  local destination="$directory/${CANONICAL_BASE}${suffix}"
  [[ -d "$directory" ]] || return 0

  local source
  source="$(find "$directory" -maxdepth 1 -type f -name "$pattern" ! -name "${CANONICAL_BASE}${suffix}" -print -quit)"
  if [[ -n "$source" ]]; then
    rm -f "$destination" "${destination}.sig"
    mv "$source" "$destination"
    if [[ -f "${source}.sig" ]]; then
      mv "${source}.sig" "${destination}.sig"
    fi
  fi
}

normalize_artifact "$BUNDLE_ROOT/appimage" '*.AppImage' '.AppImage'
normalize_artifact "$BUNDLE_ROOT/appimage" '*.AppImage.tar.gz' '.AppImage.tar.gz'
normalize_artifact "$BUNDLE_ROOT/deb" '*.deb' '.deb'
normalize_artifact "$BUNDLE_ROOT/rpm" '*.rpm' '.rpm'

echo "Linux release artifacts normalized with base name: ${CANONICAL_BASE}"
