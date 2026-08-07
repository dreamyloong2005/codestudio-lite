#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "macOS updater packaging must run on macOS." >&2
  exit 1
fi

npm run build

for target_and_arch in \
  "aarch64-apple-darwin:arm64" \
  "x86_64-apple-darwin:x64"; do
  target="${target_and_arch%%:*}"
  bash "$ROOT_DIR/scripts/build-macos-updater-target.sh" "$target" --frontend-ready "$@"
done
