---
"openrscad-release-root": minor
---

Primitives with a non-positive dimension now produce no geometry, matching OpenSCAD: zero or negative `cube`/`square` components, `sphere`/`circle` radii, `cylinder` height or radii, and `linear_extrude` height. Previously these built solids — `cube([-2,3,4])` had volume 24 — and the zero-size cases emitted non-manifold triangles that could make an enclosing `difference()` or `union()` fail in the CSG kernel. A single zero `cylinder` radius is still a valid cone.
