#!/usr/bin/env bash
set -euo pipefail

# smoke-test.sh <dmg>
# Installs the app from the release DMG the way a user would and fails unless it
# passes gatekeeper (when signed), runs every default scene, exports a frame and
# stays up as a GUI.

DMG="${1:?usage: $0 <dmg>}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
source "$ROOT/bundling/smoke-test-common.sh"

work="$(mktemp -d)"
mount="$work/mnt"
trap 'hdiutil detach -quiet "$mount" 2>/dev/null || true; rm -rf "$work"' EXIT

hdiutil attach -nobrowse -readonly -mountpoint "$mount" "$DMG" >/dev/null
cp -R "$mount/Monocurl.app" "$work/"
hdiutil detach -quiet "$mount"

APP="$work/Monocurl.app"
BIN="$APP/Contents/MacOS/monocurl"
SCENES="$APP/Contents/Resources/assets/default_scenes"

if [[ -n "${CODESIGN_IDENTITY:-}" ]]; then
    codesign --verify --deep --strict "$APP"
    spctl --assess --type execute --verbose "$APP"
fi

# keep the app away from any real settings or window state on the runner
export HOME="$work/home"
mkdir -p "$HOME"

smoke_scenes "$BIN" "$SCENES"
smoke_image "$BIN" "$SCENES/(Example) Shader.mcs" "$work/frame.png"
smoke_gui "$BIN" "$work/gui.log"
echo "[ok] macOS smoke test passed"
