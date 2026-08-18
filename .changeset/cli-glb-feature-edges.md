---
"openrscad-release-root": patch
---

`openrscad` writes `.glb` from the `-o` flag, carrying the authored scene hierarchy and per-owner materials, and `--edges` adds source-derived feature lines to it. Feature-edge lines are opaque black rather than translucent grey, so a renderer that consumes the file as written draws them the same way the desktop preview does. A model that renders to nothing now exports a valid empty document instead of one with empty glTF arrays, and zero-area faces export a unit normal instead of a zero vector.
