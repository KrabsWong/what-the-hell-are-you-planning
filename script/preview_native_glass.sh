#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --features qa --locked --offline
preview_bundle="$PWD/target/debug/PlanningGlassPreview.app"
preview_binary="$preview_bundle/Contents/MacOS/what-the-hell-are-you-planning"
# Only replace this isolated preview; never stop the normal Planning app.
while IFS= read -r preview_pid; do
  [[ -z "$preview_pid" ]] || kill "$preview_pid"
done < <(pgrep -f "^${preview_binary}( |$)" || true)
mkdir -p "$preview_bundle/Contents/MacOS" "$preview_bundle/Contents/Resources" .local/native-glass
cp target/debug/what-the-hell-are-you-planning "$preview_binary"
cp assets/brand/macos/Info.plist "$preview_bundle/Contents/Info.plist"
cp assets/brand/macos/Planning.icns "$preview_bundle/Contents/Resources/Planning.icns"
/usr/libexec/PlistBuddy -c 'Set :CFBundleIdentifier com.krabswong.planning.glass-preview' "$preview_bundle/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Set :CFBundleName Planning Glass Preview' "$preview_bundle/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Set :CFBundleDisplayName Planning Glass Preview' "$preview_bundle/Contents/Info.plist"
/usr/bin/codesign --force --sign - "$preview_bundle"
/usr/bin/codesign --verify --strict "$preview_bundle"
/usr/bin/open -n --stdout "$PWD/.local/native-glass/launch.log" --stderr "$PWD/.local/native-glass/launch-error.log" "$preview_bundle" --args --qa-native-glass "$@"
