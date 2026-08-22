---
"openrscad-release-root": minor
---

The retained deprecated OpenSCAD 2021.01 forms now work: `assign()`, `child()`, `import_stl()`, `import_dxf()`, `dxf_dim()` and `dxf_cross()`, each with the deprecation notice OpenSCAD prints. Two behaviours the names mislead about are pinned by oracle cases: `assign()` is not `let()` — its right-hand sides all evaluate in the enclosing scope, so `x = 100; assign(x = 1, y = x + 1)` gives `y == 101` — and bare `child()` selects only the first child where bare `children()` selects all of them.
