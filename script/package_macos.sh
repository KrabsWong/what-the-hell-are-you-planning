#!/bin/bash
# Produce a self-contained, ad-hoc signed Apple Silicon beta DMG.
set -euo pipefail
cd "$(dirname "$0")/.."
[[ "$(uname -s)" == Darwin && "$(uname -m)" == arm64 ]] || {
  echo 'This packaging script currently supports Apple Silicon macOS only.' >&2
  exit 1
}
# Requires Python 3.10+; this example uses Python 3.13.
# Install once with: python3.13 -m venv target/dmg-tools &&
# target/dmg-tools/bin/pip install -r script/requirements-dmg.txt
dmgbuild="$PWD/target/dmg-tools/bin/dmgbuild"
[[ -x "$dmgbuild" ]] || {
  echo 'Install the packaging tools from script/requirements-dmg.txt into target/dmg-tools first.' >&2
  exit 1
}
cargo build --release --locked --offline
version=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' assets/brand/macos/Info.plist)
build_id=$(date -u +%Y%m%d%H%M%S)
output="$PWD/target/distribution"
module_cache="$PWD/target/installer-cache/swift"
staging=$(mktemp -d "$TMPDIR/planning-package.XXXXXX")
trap 'rm -rf "$staging"' EXIT
app="$staging/Planning.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$output" "$module_cache"
cp target/release/what-the-hell-are-you-planning "$app/Contents/MacOS/"
cp assets/brand/macos/Info.plist "$app/Contents/Info.plist"
cp assets/brand/macos/Planning.icns "$app/Contents/Resources/"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $build_id" "$app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Add :LSMinimumSystemVersion string 11.0' "$app/Contents/Info.plist"
/usr/bin/plutil -lint "$app/Contents/Info.plist"
# Refuse a package that depends on developer-local dynamic libraries.
if otool -L "$app/Contents/MacOS/what-the-hell-are-you-planning" | tail -n +2 | awk '{print $1}' | grep -Ev '^(/System/Library/|/usr/lib/)' ; then
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
Open ego(lite) and complete its initial setup after installation.
Model providers may charge fees. Use your own accounts and configuration.

Signing and first launch
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
/usr/bin/codesign --force --sign - "$app"
/usr/bin/codesign --verify --strict --verbose=2 "$app"
name="Planning-${version}-beta-${build_id}-arm64"
"$dmgbuild" -s script/dmg_settings.py -D staging="$staging" "Planning Beta" "$output/$name.dmg"
hdiutil verify "$output/$name.dmg"
(cd "$output" && shasum -a 256 "$name.dmg" > "$name.dmg.sha256")
printf '\nPackage: %s\nChecksum: %s\n' "$output/$name.dmg" "$output/$name.dmg.sha256"
