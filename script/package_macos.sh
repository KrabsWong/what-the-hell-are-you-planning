#!/bin/bash
# Produce a self-contained, ad-hoc signed Apple Silicon DMG with Sparkle updates.
set -euo pipefail
cd "$(dirname "$0")/.."
[[ "$(uname -s)" == Darwin && "$(uname -m)" == arm64 ]] || {
  echo 'This packaging script currently supports Apple Silicon macOS only.' >&2
  exit 1
}
: "${SPARKLE_PUBLIC_ED_KEY:?Set SPARKLE_PUBLIC_ED_KEY to the release signing public key}"
export SPARKLE_PUBLIC_ED_KEY
python3 - <<'PY'
import base64, os
key = base64.b64decode(os.environ['SPARKLE_PUBLIC_ED_KEY'], validate=True)
if len(key) != 32:
    raise SystemExit('SPARKLE_PUBLIC_ED_KEY must encode a 32-byte Ed25519 public key')
PY
if [[ -z "${SPARKLE_FRAMEWORK_DIR:-}" ]]; then
  sdk="$(bash scripts/fetch-sparkle.sh)"
  export SPARKLE_FRAMEWORK_DIR="$sdk/Sparkle.xcframework/macos-arm64_x86_64"
fi
# Requires Python 3.10+; this example uses Python 3.13.
# Install once with: python3.13 -m venv target/dmg-tools &&
# target/dmg-tools/bin/pip install -r script/requirements-dmg.txt
dmgbuild="$PWD/target/dmg-tools/bin/dmgbuild"
[[ -x "$dmgbuild" ]] || {
  echo 'Install the packaging tools from script/requirements-dmg.txt into target/dmg-tools first.' >&2
  exit 1
}
cargo build --release --locked --features sparkle
version=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' assets/brand/macos/Info.plist)
build_id=$(date -u +%Y%m%d%H%M%S)
output="$PWD/target/distribution"
dist="$PWD/dist"
module_cache="$PWD/target/installer-cache/swift"
staging=$(mktemp -d "$TMPDIR/planning-package.XXXXXX")
trap 'rm -rf "$staging"' EXIT
app="$staging/Planning.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$app/Contents/Frameworks" \
  "$output" "$dist" "$module_cache"
cp target/release/what-the-hell-are-you-planning "$app/Contents/MacOS/"
cp assets/brand/macos/Info.plist "$app/Contents/Info.plist"
cp assets/brand/macos/Planning.icns "$app/Contents/Resources/"
ditto "$SPARKLE_FRAMEWORK_DIR/Sparkle.framework" "$app/Contents/Frameworks/Sparkle.framework"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $build_id" "$app/Contents/Info.plist"
APP_BUNDLE="$app" python3 - <<'PY'
import os, plistlib
from pathlib import Path
path = Path(os.environ['APP_BUNDLE']) / 'Contents' / 'Info.plist'
info = plistlib.loads(path.read_bytes())
info.update(
    SUFeedURL='https://github.com/KrabsWong/what-the-hell-are-you-planning/releases/latest/download/appcast.xml',
    SUPublicEDKey=os.environ['SPARKLE_PUBLIC_ED_KEY'],
    SUEnableAutomaticChecks=True, SUScheduledCheckInterval=43200,
    SUAutomaticallyUpdate=False, SUAllowsAutomaticUpdates=False,
    SUVerifyUpdateBeforeExtraction=True, SURequireSignedFeed=True,
    SUSignedFeedFailureExpirationInterval=0)
path.write_bytes(plistlib.dumps(info))
PY
/usr/bin/plutil -lint "$app/Contents/Info.plist"
# Refuse a package that depends on developer-local dynamic libraries.
if otool -L "$app/Contents/MacOS/what-the-hell-are-you-planning" | tail -n +2 | awk '{print $1}' | grep -Ev '^(/System/Library/|/usr/lib/|@rpath/Sparkle\.framework/)' ; then
  echo 'Non-system dynamic dependency found; bundle it before distributing.' >&2
  exit 1
fi
mkdir -p "$staging/.background"
cp LICENSE "$app/Contents/Resources/LICENSE.txt"
cp assets/brand/macos/Planning.icns "$staging/.VolumeIcon.icns"
swift -module-cache-path "$module_cache" script/generate_dmg_background.swift "$staging/.background/install.tiff"
cat > "$app/Contents/Resources/Installation.txt" <<'GUIDE'
Planning Beta | Apple Silicon

Installation
1. Drag Planning into Applications.
2. Eject the disk image and open Planning from Applications.
3. First launch checks for OpenCode, Codex, and ego(lite). Install at least
   one of OpenCode or Codex, then choose your own workspace.
4. After installing dependencies, click Check again, then Continue.
   Configure your agent, model, and provider in Settings.

Install dependencies separately
OpenCode: https://opencode.ai
Codex: https://developers.openai.com/codex/cli
Install at least one of OpenCode or Codex.
ego(lite): https://lite.ego.app
Complete ego(lite)'s initial setup after installation.
Model providers may charge fees. Use your own accounts and configuration.

Updates and signing
Release builds check for signed updates every 12 hours by default. You can
disable automatic checks or check manually in Settings > General > App updates.
Installation still requires your confirmation.

This beta is ad-hoc signed and has not been notarized by Apple.
macOS may block an app downloaded from the internet. Only if you trust
its distributor, follow the macOS prompts in System Settings > Privacy
& Security to allow it to open. Organization-managed Macs may prohibit
unnotarized apps; contact your administrator.
Apple guidance: https://support.apple.com/en-us/102445

Compatibility
Apple Silicon (arm64) only; Intel Macs are not supported. The minimum
build target is macOS 11. Validation covers the development machine only,
not every older macOS release.

Privacy
This package contains no developer workspace, tasks, accounts, or credentials.
Task execution uses the model providers you configure.
GUIDE
framework="$app/Contents/Frameworks/Sparkle.framework"
for service in "$framework"/Versions/B/XPCServices/*.xpc; do
  /usr/bin/codesign --force --sign - "$service"
done
/usr/bin/codesign --force --sign - "$framework/Versions/B/Autoupdate"
/usr/bin/codesign --force --sign - "$framework/Versions/B/Updater.app"
/usr/bin/codesign --force --sign - "$framework"
/usr/bin/codesign --force --sign - "$app"
/usr/bin/codesign --verify --deep --strict --verbose=2 "$app"
rm -rf "$dist/Planning.app"
cp -R "$app" "$dist/Planning.app"
name="Planning-${version}-beta-${build_id}-arm64"
"$dmgbuild" -s script/dmg_settings.py -D staging="$staging" "Planning Beta" "$output/$name.dmg"
hdiutil verify "$output/$name.dmg"
(cd "$output" && shasum -a 256 "$name.dmg" > "$name.dmg.sha256")
printf '\nPackage: %s\nChecksum: %s\n' "$output/$name.dmg" "$output/$name.dmg.sha256"
