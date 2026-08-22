---
"openrscad-release-root": minor
---

Import gains format parity across SVG, DXF and AMF/3MF. SVG is now walked as a tree, so element transforms (including nested groups and an element's own `transform=`) apply, `<use>` resolves, `<defs>` draws nothing, `display:none` hides, and `dpi=` sizes a document that gives no physical width. DXF gains `ELLIPSE` and tessellates imported curves at the caller's `$fn`/`$fa`/`$fs` instead of always the default. AMF and 3MF objects keep their own triangle index spaces — a package of two objects previously imported as two copies of the first — and 3MF `<build>` items are assembled with their transforms.
