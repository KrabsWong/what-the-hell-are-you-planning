# Third-party notices

## Beautiful Mermaid

This library ports flowchart and state parsing, sequence/class/ER/XY parsing,
text measurement, diagram layout, color generation, ELK adaptation and SVG
rendering from **Beautiful Mermaid 1.1.3**, Copyright (c) 2026 Craft Docs, under
the MIT License. The full original notice is retained in
[licenses/Beautiful-Mermaid-MIT.txt](licenses/Beautiful-Mermaid-MIT.txt) and LICENSE.
The exact npm integrity and source file checksums are recorded in UPSTREAM.json.

Source: https://github.com/lukilabs/beautiful-mermaid

The port replaces JavaScript ELK invocation with native Rust, resolves theme CSS
variables to RGB, uses local font fallbacks, validates unsupported/malformed
input, and honors ELK edge container coordinates. It is not endorsed by Craft
Docs and does not claim complete Mermaid syntax or exhaustive upstream compatibility.

## Native ELK

`elk-layout` 0.12.2 and the `org-eclipse-elk-*` crates declare
`EPL-2.0 OR GPL-3.0-or-later`. They are external dependencies, not relicensed by
this crate's MIT declaration. Their upstream license is preserved in
[licenses/ELK-LICENSE.txt](licenses/ELK-LICENSE.txt). Distribution must preserve
applicable upstream notices and source obligations for the chosen license.

Source: https://github.com/openedges/elk-rs
Upstream: https://github.com/eclipse-elk/elk

## Other Rust dependencies

`regex`, `serde`, and `serde_json` declare `MIT OR Apache-2.0`. Cargo.lock pins the
complete dependency graph. Review the resolved dependency inventory and notices
again before publishing or distributing binaries. This preview remains
`publish = false`; no community package or repository has been published.

## Development-only reference tools

Reference generation uses Beautiful Mermaid 1.1.3, ELK.js 0.11.1 (EPL-2.0), and
entities 7.0.1 (BSD-2-Clause) from an externally installed npm package. No npm
packages, JS engines, or their implementation code are bundled in the Rust
runtime. Checked-in reference JSON/SVGs are generated from the supplied fixtures.
`tools/generate_reference.mjs` is optional development tooling, not a build step.
