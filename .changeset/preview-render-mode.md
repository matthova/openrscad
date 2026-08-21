---
"openrscad-release-root": minor
---

`$preview` now follows the render path instead of always being `true`. Exact renders and mesh/DXF/SVG exports evaluate it as `false` (matching OpenSCAD's F6), while F5 preview, PNG rasters, and echo-only runs keep `true` — so a model that branches on `$preview` no longer exports its preview-only geometry. The CLI gains OpenSCAD's `--render` and `--preview` overrides.
