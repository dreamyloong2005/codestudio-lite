#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAURI_CONFIG="${DMG_TAURI_CONFIG:-"$ROOT_DIR/src-tauri/tauri.conf.json"}"
NODE_BIN="${NODE_BIN:-}"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "macOS DMG packaging must run on macOS." >&2
  exit 1
fi

APP_PATH="${1:-}"
DMG_PATH="${2:-}"

if [[ -z "$APP_PATH" || -z "$DMG_PATH" ]]; then
  echo "Usage: scripts/create-macos-dmg.sh <app-path> <dmg-path>" >&2
  exit 1
fi

if [[ ! -d "$APP_PATH" ]]; then
  echo "macOS app bundle not found: $APP_PATH" >&2
  exit 1
fi

if [[ "${DMG_PATH: -4}" != ".dmg" ]]; then
  echo "DMG output path must end with .dmg: $DMG_PATH" >&2
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

CONFIG_VALUES="$("$NODE_BIN" - "$TAURI_CONFIG" <<'JS'
const fs = require("fs");
const path = require("path");
const configPath = process.argv[2];
const config = JSON.parse(fs.readFileSync(configPath, "utf8"));
const dmg = config.bundle?.macOS?.dmg ?? {};
const productName = process.env.DMG_PRODUCT_NAME || config.productName || "CodeStudio Lite";
const volumeName = process.env.DMG_VOLUME_NAME || productName;
const windowSize = dmg.windowSize ?? {};
const windowPosition = dmg.windowPosition ?? {};
const appPosition = dmg.appPosition ?? {};
const applicationFolderPosition = dmg.applicationFolderPosition ?? {};
const background = dmg.background
  ? path.resolve(path.dirname(configPath), dmg.background)
  : "";
const values = {
  DMG_PRODUCT_NAME_RESOLVED: productName,
  DMG_VOLUME_NAME_RESOLVED: volumeName,
  DMG_WINDOW_X: windowPosition.x ?? 10,
  DMG_WINDOW_Y: windowPosition.y ?? 60,
  DMG_WINDOW_WIDTH: windowSize.width ?? 660,
  DMG_WINDOW_HEIGHT: windowSize.height ?? 400,
  DMG_APP_X: appPosition.x ?? 180,
  DMG_APP_Y: appPosition.y ?? 170,
  DMG_APPLICATION_FOLDER_X: applicationFolderPosition.x ?? 480,
  DMG_APPLICATION_FOLDER_Y: applicationFolderPosition.y ?? 170,
  DMG_BACKGROUND: background,
};

for (const [key, value] of Object.entries(values)) {
  console.log(key + "\t" + String(value));
}
JS
)"

while IFS=$'\t' read -r key value; do
  case "$key" in
    DMG_PRODUCT_NAME_RESOLVED) DMG_PRODUCT_NAME_RESOLVED="$value" ;;
    DMG_VOLUME_NAME_RESOLVED) DMG_VOLUME_NAME_RESOLVED="$value" ;;
    DMG_WINDOW_X) DMG_WINDOW_X="$value" ;;
    DMG_WINDOW_Y) DMG_WINDOW_Y="$value" ;;
    DMG_WINDOW_WIDTH) DMG_WINDOW_WIDTH="$value" ;;
    DMG_WINDOW_HEIGHT) DMG_WINDOW_HEIGHT="$value" ;;
    DMG_APP_X) DMG_APP_X="$value" ;;
    DMG_APP_Y) DMG_APP_Y="$value" ;;
    DMG_APPLICATION_FOLDER_X) DMG_APPLICATION_FOLDER_X="$value" ;;
    DMG_APPLICATION_FOLDER_Y) DMG_APPLICATION_FOLDER_Y="$value" ;;
    DMG_BACKGROUND) DMG_BACKGROUND="$value" ;;
  esac
done <<<"$CONFIG_VALUES"

: "${DMG_PRODUCT_NAME_RESOLVED:?}"
: "${DMG_VOLUME_NAME_RESOLVED:?}"
: "${DMG_WINDOW_X:?}"
: "${DMG_WINDOW_Y:?}"
: "${DMG_WINDOW_WIDTH:?}"
: "${DMG_WINDOW_HEIGHT:?}"
: "${DMG_APP_X:?}"
: "${DMG_APP_Y:?}"
: "${DMG_APPLICATION_FOLDER_X:?}"
: "${DMG_APPLICATION_FOLDER_Y:?}"
: "${DMG_BACKGROUND:=}"

APP_NAME="${DMG_PRODUCT_NAME_RESOLVED}.app"
STAGING_DIR="$(mktemp -d -t codestudio-dmg-stage.XXXXXXXX)"
WORK_DIR="$(mktemp -d -t codestudio-dmg-work.XXXXXXXX)"
RW_DMG_PATH="$WORK_DIR/rw.$(basename "$DMG_PATH")"
HYBRID_DMG_PATH="$WORK_DIR/hybrid.$(basename "$DMG_PATH")"
ACTIVE_DMG_DEVICE=""

detach_active_device() {
  if [[ -n "$ACTIVE_DMG_DEVICE" ]]; then
    /usr/bin/hdiutil detach "$ACTIVE_DMG_DEVICE" >/dev/null 2>&1 || true
    ACTIVE_DMG_DEVICE=""
  fi
}

cleanup() {
  detach_active_device
  rm -rf "$STAGING_DIR"
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

escape_applescript_string() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

stage_signed_app() {
  echo "Staging signed app for DMG..."
  rm -rf "$STAGING_DIR"
  mkdir -p "$STAGING_DIR"
  /usr/bin/ditto "$APP_PATH" "$STAGING_DIR/$APP_NAME"
  ln -s /Applications "$STAGING_DIR/Applications"
}

source_size_mb() {
  /usr/bin/du -sm "$STAGING_DIR" | awk '{print $1}'
}

mount_dir_from_attach_output() {
  awk '
    /^\/dev\// && NF >= 3 {
      mount = "";
      for (i = 3; i <= NF; i++) {
        mount = mount (i == 3 ? "" : " ") $i;
      }
      if (mount ~ /^\//) {
        print mount;
      }
    }
  '
}

create_finder_layout_script() {
  local script_path="$1"
  local escaped_app_name
  local escaped_background_name=""
  escaped_app_name="$(escape_applescript_string "$APP_NAME")"

  local background_clause=""
  local reposition_hidden_files_clause=""
  if [[ -n "$DMG_BACKGROUND" ]]; then
    escaped_background_name="$(escape_applescript_string "$(basename "$DMG_BACKGROUND")")"
    background_clause="set background picture of opts to file \".background:$escaped_background_name\""
    reposition_hidden_files_clause="set position of every item to {theBottomRightX + 100, 100}"
  fi

  cat >"$script_path" <<OSA
on run (volumeName)
  tell application "Finder"
    tell disk (volumeName as string)
      open

      set theXOrigin to $DMG_WINDOW_X
      set theYOrigin to $DMG_WINDOW_Y
      set theWidth to $DMG_WINDOW_WIDTH
      set theHeight to $DMG_WINDOW_HEIGHT

      set theBottomRightX to (theXOrigin + theWidth)
      set theBottomRightY to (theYOrigin + theHeight)
      set dsStore to "\\"" & "/Volumes/" & volumeName & "/" & ".DS_STORE\\""

      tell container window
        set current view to icon view
        set toolbar visible to false
        set statusbar visible to false
        set the bounds to {theXOrigin, theYOrigin, theBottomRightX, theBottomRightY}
        set statusbar visible to false
        $reposition_hidden_files_clause
      end tell

      set opts to the icon view options of container window
      tell opts
        set icon size to 128
        set text size to 16
        set arrangement to not arranged
      end tell
      $background_clause

      set position of item "$escaped_app_name" to {$DMG_APP_X, $DMG_APP_Y}
      set the extension hidden of item "$escaped_app_name" to true
      set position of item "Applications" to {$DMG_APPLICATION_FOLDER_X, $DMG_APPLICATION_FOLDER_Y}

      close
      open
      delay 1

      tell container window
        set statusbar visible to false
        set the bounds to {theXOrigin, theYOrigin, theBottomRightX - 10, theBottomRightY - 10}
      end tell
    end tell

    delay 1

    tell disk (volumeName as string)
      tell container window
        set statusbar visible to false
        set the bounds to {theXOrigin, theYOrigin, theBottomRightX, theBottomRightY}
      end tell
    end tell

    delay 3

    set waitTime to 0
    set ejectMe to false
    repeat while ejectMe is false
      delay 1
      set waitTime to waitTime + 1
      if (do shell script "[ -f " & dsStore & " ]; echo $?") = "0" then set ejectMe to true
    end repeat
  end tell
end run
OSA
}

create_tauri_style_dmg() {
  local image_mb
  local attach_output
  local mount_dir
  local volume_icon="$APP_PATH/Contents/Resources/icon.icns"
  local apple_script="$WORK_DIR/finder-layout.applescript"

  rm -f "$DMG_PATH" "$RW_DMG_PATH"
  image_mb=$(( $(source_size_mb) + 64 ))

  echo "Creating Tauri-style DMG with Finder layout: $DMG_PATH"
  echo "Using Tauri DMG layout: window=${DMG_WINDOW_WIDTH}x${DMG_WINDOW_HEIGHT}, app=(${DMG_APP_X},${DMG_APP_Y}), Applications=(${DMG_APPLICATION_FOLDER_X},${DMG_APPLICATION_FOLDER_Y})"

  /usr/bin/hdiutil create \
    -volname "$DMG_VOLUME_NAME_RESOLVED" \
    -srcfolder "$STAGING_DIR" \
    -fs HFS+ \
    -fsargs "-c c=64,a=16,e=16" \
    -format UDRW \
    -size "${image_mb}m" \
    "$RW_DMG_PATH"

  attach_output="$(/usr/bin/hdiutil attach -mountrandom /Volumes -readwrite -noverify -noautoopen -nobrowse "$RW_DMG_PATH")"
  ACTIVE_DMG_DEVICE="$(awk '/^\/dev\// { print $1; exit }' <<<"$attach_output")"
  mount_dir="$(mount_dir_from_attach_output <<<"$attach_output" | tail -n 1)"
  if [[ -z "$ACTIVE_DMG_DEVICE" || -z "$mount_dir" || ! -d "$mount_dir" ]]; then
    echo "Failed to mount temporary DMG for Finder layout." >&2
    echo "$attach_output" >&2
    return 1
  fi

  if [[ -f "$volume_icon" ]]; then
    /bin/cp "$volume_icon" "$mount_dir/.VolumeIcon.icns"
    /usr/bin/SetFile -c icnC "$mount_dir/.VolumeIcon.icns"
    /usr/bin/SetFile -a C "$mount_dir"
  fi

  if [[ -n "$DMG_BACKGROUND" ]]; then
    if [[ ! -f "$DMG_BACKGROUND" ]]; then
      echo "Configured DMG background was not found: $DMG_BACKGROUND" >&2
      return 1
    fi
    mkdir -p "$mount_dir/.background"
    /bin/cp "$DMG_BACKGROUND" "$mount_dir/.background/$(basename "$DMG_BACKGROUND")"
  fi

  create_finder_layout_script "$apple_script"
  /bin/sleep 2
  /usr/bin/osascript "$apple_script" "$(basename "$mount_dir")"
  /bin/sleep 4

  /bin/chmod -Rf go-w "$mount_dir" >/dev/null 2>&1 || true
  /bin/rm -rf "$mount_dir/.fseventsd" || true

  /usr/bin/hdiutil detach "$ACTIVE_DMG_DEVICE"
  ACTIVE_DMG_DEVICE=""

  /usr/bin/hdiutil convert \
    "$RW_DMG_PATH" \
    -format UDZO \
    -imagekey zlib-level=9 \
    -o "$DMG_PATH"
}

create_plain_dmg_fallback() {
  echo "Creating plain sandbox fallback DMG: $DMG_PATH" >&2
  echo "WARNING: this fallback does not contain Tauri/Finder window layout UI." >&2
  rm -f "$DMG_PATH" "$HYBRID_DMG_PATH"
  /usr/bin/hdiutil makehybrid \
    -hfs \
    -default-volume-name "$DMG_VOLUME_NAME_RESOLVED" \
    -o "$HYBRID_DMG_PATH" \
    "$STAGING_DIR"
  /usr/bin/hdiutil convert \
    "$HYBRID_DMG_PATH" \
    -format UDZO \
    -imagekey zlib-level=9 \
    -o "$DMG_PATH"
}

mkdir -p "$(dirname "$DMG_PATH")"
stage_signed_app

set +e
create_tauri_style_dmg
CREATE_STATUS=$?
set -e

if [[ $CREATE_STATUS -ne 0 ]]; then
  detach_active_device
  if [[ "${DMG_ALLOW_PLAIN_FALLBACK:-0}" == "1" ]]; then
    create_plain_dmg_fallback
  else
    cat >&2 <<EOF
Tauri-style DMG creation failed.
This path must mount a temporary DMG and let Finder write the .DS_Store layout.
If you are running inside the Codex desktop sandbox, rerun the build from a normal Terminal to create the release DMG UI.
For a non-release smoke test only, set DMG_ALLOW_PLAIN_FALLBACK=1 to create a plain DMG without Finder layout.
EOF
    exit "$CREATE_STATUS"
  fi
fi

/usr/bin/hdiutil verify "$DMG_PATH"
/usr/bin/shasum -a 256 "$DMG_PATH"
