#!/bin/bash
# Builds target/dmg/Shor-<version>.dmg on a Mac: Shor.app for both Apple
# silicon and Intel, with the samples that ship (the git-tracked files in
# assets/ - not a local sample library).
#
# Unsigned by default: on first open macOS says it can't check the app.
# Open it once, then System Settings > Privacy & Security > Open Anyway.
#
# With a Developer ID it's signed and notarized, and opens without that:
#   SIGN_ID="Developer ID Application: Your Name (TEAMID)" \
#   NOTARY_PROFILE=shor scripts/make-dmg.sh
# (NOTARY_PROFILE is a keychain profile made once with
#  `xcrun notarytool store-credentials shor`.)
set -euo pipefail
cd "$(dirname "$0")/.."

[[ "$(uname)" == Darwin ]] || { echo "Run this on a Mac." >&2; exit 1; }

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' ui/Cargo.toml | head -1)
OUT=target/dmg
APP="$OUT/Shor.app"

rustup target add aarch64-apple-darwin x86_64-apple-darwin
for target in aarch64-apple-darwin x86_64-apple-darwin; do
    MACOSX_DEPLOYMENT_TARGET=11.0 cargo build --release -p ui --target "$target"
done

rm -rf "$OUT"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/Shor" \
    target/aarch64-apple-darwin/release/ui target/x86_64-apple-darwin/release/ui

# Samples: Contents/Resources/assets, where paths.rs looks in a bundle.
git ls-files -z assets | while IFS= read -r -d '' file; do
    mkdir -p "$APP/Contents/Resources/$(dirname "$file")"
    cp "$file" "$APP/Contents/Resources/$file"
done

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Shor</string>
    <key>CFBundleDisplayName</key><string>Shor</string>
    <key>CFBundleIdentifier</key><string>app.shor.Shor</string>
    <key>CFBundleExecutable</key><string>Shor</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>LSApplicationCategoryType</key><string>public.app-category.music</string>
    <key>NSMicrophoneUsageDescription</key><string>Shor records from your microphone or audio interface onto audio tracks.</string>
</dict>
</plist>
PLIST

# Recording needs the microphone entitlement once the app is signed with
# the hardened runtime (notarization requires that runtime).
cat > "$OUT/entitlements.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.security.device.audio-input</key><true/>
</dict>
</plist>
PLIST

if [[ -n "${SIGN_ID:-}" ]]; then
    codesign --force --options runtime --timestamp --entitlements "$OUT/entitlements.plist" --sign "$SIGN_ID" "$APP"
else
    # Ad hoc: not trusted, but Apple silicon won't run an unsigned app at
    # all ("damaged"), and this makes it the usual "can't check" warning.
    codesign --force --sign - "$APP"
fi

# The disk image: the app beside a link to Applications, to drag onto.
STAGE="$OUT/stage"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
DMG="$OUT/Shor-$VERSION.dmg"
hdiutil create -volname "Shor" -srcfolder "$STAGE" -ov -format UDZO "$DMG"
rm -rf "$STAGE"

if [[ -n "${SIGN_ID:-}" ]]; then
    codesign --force --timestamp --sign "$SIGN_ID" "$DMG"
    if [[ -n "${NOTARY_PROFILE:-}" ]]; then
        xcrun notarytool submit "$DMG" --keychain-profile "$NOTARY_PROFILE" --wait
        xcrun stapler staple "$DMG"
    fi
fi

echo "Built $DMG"
