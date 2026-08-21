---
"openrscad-release-root": minor
---

`linear_extrude` now refines for a non-uniform `scale` the way OpenSCAD does, re-tessellating the profile and adding slices — a scaled square exports 596 triangles instead of 12, matching upstream. A uniform scale keeps the walls planar and is still left unrefined. This is volume-neutral, so the new oracle cases pin triangle counts as well as volume.
