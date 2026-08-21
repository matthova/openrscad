---
"openrscad-release-root": patch
---

Fix stray sliver triangles on the web/desktop-wasm build's CSG output. The pure-Rust Manifold kernel used on `wasm32` (`manifold-rust`) was upgraded 0.9.2 → 0.13.1, which cleans up the triangulation of complex coplanar faces (e.g. a tray top punched with many pockets). Previously such faces could emit zero-area, collinear "spear" triangles and dashed sliver artifacts — most visible on models like the Ultimate Parametric Battery Organizer. The mesh is now closer to the native C++ kernel's (fewer, non-degenerate triangles) with an identical bounding box and volume.
