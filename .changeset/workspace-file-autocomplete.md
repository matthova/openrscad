---
"openrscad-release-root": minor
---

The playground editor now autocompletes the names of other files in your workspace. Inside `include <…>` / `use <…>` it offers your sibling `.scad` files, and inside `import("…")` / `surface("…")` (including the `file="…"` form) it offers your imported assets (STL, SVG, DXF, 3MF, …) — so a second file added to a project is one keystroke away from being referenced in the main file.
