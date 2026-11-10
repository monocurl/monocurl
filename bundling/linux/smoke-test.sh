#!/usr/bin/env bash
set -euo pipefail

# smoke-test.sh <tarball>
# Unpacks the release tarball and fails unless the bundled app runs every
# default scene, exports a frame and stays up as a GUI. Expects a display
# (xvfb-run) and a vulkan driver (lavapipe on CI).

TARBALL="${1:?usage: $0 <tarball>}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
source "$ROOT/bundling/smoke-test-common.sh"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
tar -xzf "$TARBALL" -C "$work"

APP="$work/monocurl.app"
BIN="$APP/bin/monocurl"
SCENES="$APP/assets/default_scenes"

export HOME="$work/home"
mkdir -p "$HOME"

smoke_scenes "$BIN" "$SCENES"
smoke_image "$BIN" "$SCENES/(Example) Shader.mcs" "$work/frame.png"
smoke_gui "$BIN" "$work/gui.log"
echo "[ok] Linux smoke test passed"
