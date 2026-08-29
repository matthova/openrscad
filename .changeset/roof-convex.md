---
"openrscad-release-root": minor
---

add `roof()` for convex profiles — lifts a 2D outline to its straight-skeleton roof (every point rises at unit slope to the ridge). Squares, rectangles, triangles, and regular polygons match OpenSCAD exactly and produce a manifold solid usable in booleans. Concave profiles and profiles with holes need split events and are warned and skipped for now. Like OpenSCAD's experimental `roof()`, but enabled unconditionally.
