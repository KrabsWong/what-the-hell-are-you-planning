# Validation record

Date: 2026-11-11. Scope: six-family native diagram library. The parent application
now uses it for reader/report rendering; app build and visual checks are recorded
separately from library geometry and raster checks.

## Original flowchart baseline checks

- `cargo test --release --offline`: 18 unit tests, 4 integration tests, and 1
  documentation example passed.
- `cargo clippy --release --offline --all-targets -- -D warnings`: passed.
- `cargo package --allow-dirty --offline`: source archive created and extracted
  package compiled successfully. Build/output artifacts are excluded explicitly.
- Five pinned reference fixtures match upstream ELK input with numeric tolerance
  1e-9: gates, cycle, nested, direction, styles.
- Four positioned fixtures match upstream with 0.001 pixel tolerance: gates,
  cycle, direction, styles. This tolerance covers floating-point differences,
  not different layout decisions.
- Nested graphs additionally verify every node/group membership, child bounds,
  connected source/target endpoints, finite coordinates, and canvas bounds.
- Unsupported nested direction overrides fail explicitly in a regression test.
- Other tests cover text metrics, escaping, XML decode, nonfinite options,
  unsupported syntax, incomplete groups, arrow styles and pre-expansion edge
  limits. Source/target Cartesian products are bounded before allocation.

## Integrated six-family checks

The main agent ran `cargo test --release --offline`: 22 unit tests, 27 integration
tests, and one documentation example passed (50 total). The final
`cargo clippy --release --offline --all-targets -- -D warnings` also passed.
`cargo package --allow-dirty --offline` also passed after the six-family expansion: the archive contains 85 files and the extracted package builds successfully.

## Additional-family checks

The retained integration suites cover state pseudostates/composites, sequence
messages/activations/notes/blocks, class members/relationship orientations, ER
keys/cardinalities, and horizontal/vertical XY series. Pinned positioned oracles
cover simple states, participant-only sequence layout, class, ER, and both XY
orientations. Sequence actor/note/divider corrections are tested as intentional
differences rather than asserted equal to upstream.

The class/ER lane executed `cargo test --release --offline --test class_er`:
all six tests passed. It also executed `cargo clippy --release --offline --lib --
-D warnings` successfully. These are scoped checks, not a final integrated test
count or a new package-validation claim.

## Raster evidence

The optional reference-normalization tool resolves upstream theme CSS to concrete
8-bit sRGB, removes remote font imports, uses the same local fallback fonts, and
rasterizes both SVGs with the same `rsvg-convert` binary. ImageMagick computes
pixel RMSE. This checks the port's shape/text/color behavior on this machine; it
does not assert identical output with a downloaded Inter font in a browser.

| Fixture | Theme | Pixel RMSE | Result |
|---|---|---|---|
| gates | zinc-light | 0 | Identical normalized raster |
| gates | zinc-dark | 0 | Identical normalized raster |
| cycle | zinc-dark | 0 | Identical normalized raster |
| styles | zinc-dark | 0 | Identical normalized raster |
| nested | zinc-dark | Nonzero, expected | Different native placement; container coordinate fix |
| state | zinc-dark | 0 | Identical normalized raster |
| sequence-basic | zinc-dark | 0 | Identical normalized raster |
| class | zinc-light / zinc-dark | 0 | Identical normalized raster |
| er | zinc-light / zinc-dark | 0 | Identical normalized raster |
| xychart | zinc-light / zinc-dark | 0 | Identical normalized raster |
| xychart-horizontal | zinc-dark | 0 | Identical normalized raster |

Both gate themes, the nested native image, and class/ER dark images were
inspected. The nested native image has attached cross-boundary arrows and legible labels; the pinned upstream
image has detached cross-boundary paths. Raster reports and images are generated
under ignored `output/comparison/`; the source/reference fixtures are retained.

## Intentional differences and unresolved scope

1. **Nested ELK placement:** Native `elk-layout` 0.12.2 and ELK.js 0.11.1 do not
   always choose identical nested node coordinates even with identical inputs.
   Do not advertise full nested coordinate parity.
2. **Container coordinates:** The Rust adapter honors the edge `container`
   origin. This corrects detached paths in the pinned upstream nested fixture.
3. **Roundoff robustness:** Collinear bend alignment uses a 1e-6 tolerance instead
   of exact floating equality. This preserves endpoint/bend alignment after the
   native ELK JSON round trip.
4. **Unsupported routes:** Missing routes, multiple sections, and nested-level
   direction overrides return errors. Top-level direction overrides are covered.
5. **Native themes:** CSS variables and mixes resolve to RGB; local fonts replace
   remote imports. Colors may differ by at most half a channel unit from an
   unquantized mix. Arbitrary CSS expressions are deliberately unsupported.
6. **Sequence corrections:** Actor labels, initial/multiline notes, and overlapping
   block dividers receive additional clearance. Block tabs paint above activation bars. These fixtures intentionally
   differ from upstream geometry to preserve readable content.
7. **Syntax coverage:** All six upstream families have implementations, but this
   is still a preview subset. Unsupported state directives, class namespaces,
   nested sequence control blocks, sequence autonumber/standalone activation/cross arrowheads, and categorical
   XY y axes fail explicitly. XY data validation is stricter than upstream's
   permissive numeric parsing. See README.md for family limits.
8. **Remaining validation:** Actual font shaping, exhaustive Unicode parity,
   fuzzing, resource cancellation, and cross-platform visual CI remain. The
   app retains its prior renderer as a fallback, so app acceptance is distinct
   from acceptance by this library.

## Review

The initial flowchart implementation was split across independent layout,
text-metrics, and SVG/theme lanes. Subsequent work used separate sequence,
class/ER, and XY lanes with main-agent state and app integration. The original
read-only review found and prompted fixes for Cartesian expansion before limits,
mixed arrow-operator styles, and XML entity decoding. The main agent ran the
integrated tests, Clippy and final packaging validation. A subsequent read-only review prompted regression fixes for state declarations after transitions and authored IDs colliding with pseudo-state IDs.

## Application integration

The reader and HTML report exporter share native-first rendering with the previous
Rust renderer retained for compatibility. App-only color overrides increase
muted label, connector and border contrast; library presets remain unchanged.
With the final dependency lock, the full application suite passed: 747 main tests
and two integration tests, with two pre-existing ignored tests. An earlier parallel
run encountered a workspace-lock test failure; the complete serial rerun passed.
The isolated GPUI QA generated light, dark and narrow frames for all six diagram
families plus the existing math/Markdown sample (21 frames).
