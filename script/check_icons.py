#!/usr/bin/env python3
"""Catch Lucide references that compile but are missing from the embedded bundle."""
from pathlib import Path
import re
import sys

root = Path(__file__).resolve().parents[1]
source = (root / "src/ui_assets.rs").read_text()
bundle = re.search(r"icon_assets!\(\s*AppIcons,\s*\[(.*?)\]", source, re.S)
if bundle is None:
    sys.exit("Cannot find AppIcons registry")
embedded = set(re.findall(r"\b[A-Z]\w*\b", bundle[1]))
missing = []
for path in sorted((root / "src").glob("*.rs")):
    for number, line in enumerate(path.read_text().splitlines(), 1):
        for icon in re.findall(r"IconName::(\w+)", line):
            if icon not in embedded:
                missing.append(f"{path.relative_to(root)}:{number}: embed {icon} in AppIcons")
if missing:
    sys.exit("\n".join(missing))
print(f"All explicit Lucide references are embedded ({len(embedded)} registered icons).")
