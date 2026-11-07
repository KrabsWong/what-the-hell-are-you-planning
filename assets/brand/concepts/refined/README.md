# Mark and sequence refinements

The original concept is preserved; the new version lives in this directory. Open `preview.html` to compare the original, the refined version, 16/24/32/48 px sizes and a sidebar mock, and to switch monochrome, background and animation modes.

- Mark: 20-unit stroke with 12-unit dot spacing; no square rotation, replaced by a 3-unit shift and 2-unit landing confirmation in an 8-second loop.
- Sequence: 20-unit modules with 6-unit seams; expand 3 units in order and return, in a 9-second loop.
- Animated SVG supports `prefers-reduced-motion`; static SVG has no animation styling.
- No application code changed; integrating the animation into the native renderer still needs separate implementation or verification.
