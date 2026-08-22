---
"openrscad-release-root": minor
---

`import()` now honours its selectors and placement instead of silently ignoring them: `layer=` keeps a single DXF layer or Inkscape SVG layer, `id=` selects an SVG element by id, and `origin`/`scale` place a 2D import as `(point - origin) * scale`. A selective import previously returned the entire drawing untransformed.
