#!/usr/bin/env bash
set -euo pipefail

# post-bundle.sh <version> <target>
# Run after: cargo bundle --package monocurl --release --target <target>
# Handles the parts cargo-bundle doesn't: assets, dylibs, signing, DMG, notarization.
#
# Signing (all optional — omit for ad-hoc signing):
#   CODESIGN_IDENTITY               developer id cert identity
#   CODESIGN_ENTITLEMENTS           path to entitlements file
#   APPLE_NOTARY_KEYCHAIN_PROFILE   keychain profile (preferred for local use)
#   APPLE_NOTARY_APPLE_ID       \
#   APPLE_NOTARY_PASSWORD        >  all three required when not using keychain profile
#   APPLE_NOTARY_TEAM_ID        /

VERSION="${1:?usage: $0 <version> <target>}"
TARGET="${2:?usage: $0 <version> <target>}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
APP="$ROOT/target/$TARGET/release/bundle/osx/Monocurl.app"
FWDIR="$APP/Contents/Frameworks"

[[ -d "$APP" ]] || { echo "[error] app bundle not found: $APP" >&2; exit 1; }
EXE_NAME="$(/usr/libexec/PlistBuddy -c "Print :CFBundleExecutable" "$APP/Contents/Info.plist")"
EXE="$APP/Contents/MacOS/$EXE_NAME"
[[ -f "$EXE" ]] || { echo "[error] app executable not found: $EXE" >&2; exit 1; }
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" "$APP/Contents/Info.plist" 2>/dev/null \
    || /usr/libexec/PlistBuddy -c "Add :CFBundleShortVersionString string $VERSION" "$APP/Contents/Info.plist"

# ---- assets -----------------------------------------------------------------
mkdir -p "$APP/Contents/Resources"
rm -rf "$APP/Contents/Resources/assets"
mkdir -p "$APP/Contents/Resources/assets"
cp -pR "$ROOT/assets/." "$APP/Contents/Resources/assets/"
find "$APP/Contents/Resources/assets" -name .DS_Store -delete

# ---- icns -------------------------------------------------------------------
SRC="$ROOT/assets/AppIcon.appiconset"
ICON_FILE="AppIcon.icns"
ICON="$APP/Contents/Resources/$ICON_FILE"
perl "$ROOT/bundling/macos/make_icns.pl" "$SRC" "$ICON"
/usr/libexec/PlistBuddy -c "Set :CFBundleIconFile $ICON_FILE" "$APP/Contents/Info.plist" 2>/dev/null \
    || /usr/libexec/PlistBuddy -c "Add :CFBundleIconFile string $ICON_FILE" "$APP/Contents/Info.plist"

# ---- dylibs -----------------------------------------------------------------
rm -rf "$FWDIR"
python3 "$ROOT/bundling/macos/bundle_dylibs.py" "$EXE" "$FWDIR"

# ---- sign -------------------------------------------------------------------
if [[ -n "${CODESIGN_IDENTITY:-}" ]]; then
    ENTITLEMENTS="${CODESIGN_ENTITLEMENTS:-$ROOT/bundling/macos/Monocurl.entitlements}"
    if ! security find-identity -v -p codesigning | grep -F "$CODESIGN_IDENTITY" >/dev/null; then
        echo "[error] codesign identity not found in keychain: $CODESIGN_IDENTITY" >&2
        echo "[error] set MACOS_CERTIFICATE_P12 and MACOS_CERTIFICATE_PASSWORD, or clear CODESIGN_IDENTITY" >&2
        security find-identity -v -p codesigning >&2 || true
        exit 1
    fi
    xattr -cr "$APP"
    find "$FWDIR" -name '*.dylib' -print0 \
        | xargs -0 -I{} codesign --force --timestamp --options runtime --sign "$CODESIGN_IDENTITY" {}
    codesign --force --timestamp --options runtime \
        --entitlements "$ENTITLEMENTS" --sign "$CODESIGN_IDENTITY" "$APP"
    codesign --verify --strict "$APP"
else
    xattr -cr "$APP" 2>/dev/null || true
    find "$FWDIR" -name '*.dylib' -print0 | xargs -0 -I{} codesign --force --sign - {}
    codesign --force --sign - "$APP"
fi

# ---- DMG --------------------------------------------------------------------
mkdir -p "$ROOT/dist/macos"
DMG="$ROOT/dist/macos/Monocurl-macos-${TARGET%%-*}.dmg"
stage="$(mktemp -d)"; trap 'rm -rf "$stage"' EXIT
cp -pR "$APP" "$stage/"
find "$stage/Monocurl.app" -name .DS_Store -delete
ln -s /Applications "$stage/Applications"

hdiutil create -volname Monocurl -srcfolder "$stage" -ov -format UDZO "$DMG"

# ---- notarize ---------------------------------------------------------------
if [[ -n "${APPLE_NOTARY_KEYCHAIN_PROFILE:-}" ]]; then
    xcrun notarytool submit "$DMG" --wait --keychain-profile "$APPLE_NOTARY_KEYCHAIN_PROFILE"
    xcrun stapler staple "$DMG"
elif [[ -n "${APPLE_NOTARY_APPLE_ID:-}" && -n "${APPLE_NOTARY_PASSWORD:-}" && -n "${APPLE_NOTARY_TEAM_ID:-}" ]]; then
    xcrun notarytool submit "$DMG" --wait \
        --apple-id "$APPLE_NOTARY_APPLE_ID" \
        --password "$APPLE_NOTARY_PASSWORD" \
        --team-id "$APPLE_NOTARY_TEAM_ID"
    xcrun stapler staple "$DMG"
fi

echo "[ok] $DMG"
