#!/usr/bin/env bash
# Packs a release build of Vortex into Vortex.app and zips it as the release
# asset the in-app updater looks for: Vortex-<version>-macos-<arch>.zip.
#
#   scripts/bundle-macos.sh <target> <version> <out-dir>
#   e.g. scripts/bundle-macos.sh aarch64-apple-darwin 0.2.0 dist
set -euo pipefail

target="$1"
version="$2"
out="$3"

case "$target" in
  aarch64-apple-darwin) arch=arm64 ;;
  x86_64-apple-darwin) arch=x86_64 ;;
  *) echo "unsupported target: $target" >&2; exit 1 ;;
esac

binary="target/$target/release/vortex"
[ -x "$binary" ] || { echo "build $binary first" >&2; exit 1; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
app="$work/Vortex.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/vortex"
if [ -f assets/macos/Vortex.icns ]; then
  cp assets/macos/Vortex.icns "$app/Contents/Resources/Vortex.icns"
fi

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Vortex</string>
  <key>CFBundleDisplayName</key><string>Vortex</string>
  <key>CFBundleIdentifier</key><string>io.github.batuhanaksoyy.vortex</string>
  <key>CFBundleExecutable</key><string>vortex</string>
  <key>CFBundleIconFile</key><string>Vortex</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

# Ad hoc: Apple silicon runs no unsigned code. Without a Developer ID the
# first start still needs Open from the app's context menu.
codesign --force --deep --sign - "$app"

mkdir -p "$out"
ditto -c -k --keepParent "$app" "$out/Vortex-$version-macos-$arch.zip"
echo "$out/Vortex-$version-macos-$arch.zip"
