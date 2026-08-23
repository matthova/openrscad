---
"openrscad-release-root": patch
---

The desktop app can now import 2D profiles and text meshes. A new **File ▸ Import File…** menu item (⌘I) and drag-and-drop both add SVG, DXF, `.scad`, OFF, OBJ, AMF, and other text files as tabs you can reference with `import("file.svg")`. Previously the desktop app's Open dialog only accepted `.scad` and native file drops were silently swallowed, so there was no way to bring an SVG into a desktop model.
