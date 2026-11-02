#!/bin/sh
# Builds the macOS installer package: the app, and BlackHole 2ch as a choice that is ticked when
# the cable is missing. A .dmg has no install step, so this is the only place to set up the cable
# before the app first opens.
#
# Usage: build-pkg.sh <InstantPad.app> <version> <arm64|x86_64> <output.pkg>
set -eu

here=$(cd "$(dirname "$0")" && pwd)
app=$1
version=$2
arch=$3
output=$4

identifier=$(/usr/libexec/PlistBuddy -c "Print CFBundleIdentifier" "$app/Contents/Info.plist")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

mkdir "$work/root"
ditto "$app" "$work/root/$(basename "$app")"
pkgbuild --analyze --root "$work/root" "$work/components.plist"
# Always to /Applications, even when a copy of the app sits somewhere else on the disk.
plutil -replace 0.BundleIsRelocatable -bool NO "$work/components.plist"
pkgbuild --root "$work/root" --component-plist "$work/components.plist" \
  --identifier "$identifier" --version "$version" --install-location /Applications \
  "$work/app.pkg"
pkgbuild --nopayload --scripts "$here/blackhole" \
  --identifier "$identifier.blackhole" --version "$version" \
  "$work/blackhole.pkg"

sed -e "s/@IDENTIFIER@/$identifier/g" -e "s/@ARCH@/$arch/g" \
  "$here/distribution.xml" > "$work/distribution.xml"
productbuild --distribution "$work/distribution.xml" --package-path "$work" \
  --resources "$here/resources" "$output"
