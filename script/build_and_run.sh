#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build
app_binary="$PWD/target/debug/what-the-hell-are-you-planning"
# Only stop this checkout's application after the new build succeeds.
while IFS= read -r app_pid; do
  [[ -z "$app_pid" ]] || kill "$app_pid"
done < <(pgrep -f "^${app_binary}$" || true)
# Earlier development launches used a relative path; match their cwd too.
while IFS= read -r app_pid; do
  if lsof -a -p "$app_pid" -d cwd -Fn 2>/dev/null | grep -Fxq "n$PWD"; then
    kill "$app_pid"
  fi
done < <(pgrep -f '^\./target/debug/what-the-hell-are-you-planning$' || true)
if [[ "$(uname -s)" == "Darwin" ]]; then
  app_bundle="$PWD/target/debug/Planning.app"
  bundled_binary="$app_bundle/Contents/MacOS/what-the-hell-are-you-planning"
  while IFS= read -r app_pid; do
    [[ -z "$app_pid" ]] || kill "$app_pid"
  done < <(pgrep -f "^${bundled_binary}$" || true)
  mkdir -p "$app_bundle/Contents/MacOS" "$app_bundle/Contents/Resources"
  cp "$app_binary" "$bundled_binary"
  cp assets/brand/macos/Info.plist "$app_bundle/Contents/Info.plist"
  cp assets/brand/macos/Planning.icns "$app_bundle/Contents/Resources/Planning.icns"
  /usr/bin/plutil -lint "$app_bundle/Contents/Info.plist"
  /usr/bin/open -n "$app_bundle"
else
  exec "$app_binary"
fi
