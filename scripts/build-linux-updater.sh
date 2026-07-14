#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "Linux updater packaging must run on Linux." >&2
  exit 1
fi

target_triple=""
arguments=("$@")
for ((index = 0; index < ${#arguments[@]}; index += 1)); do
  case "${arguments[$index]}" in
    --target)
      if ((index + 1 < ${#arguments[@]})); then
        target_triple="${arguments[$((index + 1))]}"
      fi
      ;;
    --target=*)
      target_triple="${arguments[$index]#--target=}"
      ;;
  esac
done

npm run tauri:build:updater -- "$@"
bash "$ROOT_DIR/scripts/normalize-linux-artifacts.sh" "$target_triple"
