---
"openrscad-release-root": minor
---

The CLI now rejects an unrecognized or missing `-o` suffix instead of silently writing binary STL under that name: `openrscad -o out.foo model.scad` exits non-zero and writes nothing, matching OpenSCAD. `-o out.csg` reports that CSG tree export is not supported yet rather than producing STL bytes named `.csg`. Suffixes match case-insensitively, and the check runs before evaluation so a typo does not cost a full render.
