#!/bin/bash
# Set the app version from a release tag. The tag is the single source of truth:
# CI calls this before building so the checked-out files always match the tag.
set -euo pipefail
cd "$(dirname "$0")/.."

version="${1:-}"
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "usage: script/set_version.sh MAJOR.MINOR.PATCH" >&2
  exit 1
fi

python3 - "$version" <<'PY'
import re
import sys
from pathlib import Path

version = sys.argv[1]
path = Path("Cargo.toml")
text = path.read_text(encoding="utf-8")
updated, count = re.subn(r'version = "[^"]+"', f'version = "{version}"', text, count=1)
if count != 1:
    raise SystemExit("could not update the package version in Cargo.toml")
path.write_text(updated, encoding="utf-8")
PY

python3 - "$version" <<'PY'
import re
import sys
from pathlib import Path

version = sys.argv[1]
path = Path("assets/brand/macos/Info.plist")
text = path.read_text(encoding="utf-8")
for key in ("CFBundleShortVersionString", "CFBundleVersion"):
    pattern = rf'(<key>{key}</key><string>)[^<]*(</string>)'
    text, count = re.subn(pattern, rf'\g<1>{version}\g<2>', text, count=1)
    if count != 1:
        raise SystemExit(f"could not update {key} in Info.plist")
path.write_text(text, encoding="utf-8")
PY

# The root package version is also stored in Cargo.lock, and later build steps
# use --locked, so refresh it here without compiling or the lockfile guard.
cargo metadata --format-version 1 > /dev/null

echo "Version set to $version"
