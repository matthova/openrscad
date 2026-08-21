---
"openrscad-release-root": minor
---

`linear_extrude` now matches OpenSCAD's refinement rules when twisting: `segments=` is honoured instead of ignored, an omitted `slices` follows `$fn`/`$fa`/`$fs` instead of the twist angle alone, and the 2D profile is re-tessellated before the sweep. Twisted models previously came out 6–16% off in volume — a `twist=90, $fa=3, $fs=0.5` square was 16.4% high, and negative twists were 10% low.
