---
"openrscad-release-root": minor
---

`-o out.csg` now exports the evaluated model as an OpenSCAD `.csg` operation tree — every module call resolved, every expression evaluated, every transform lowered to a `multmatrix` — instead of reporting the format unsupported. It needs no render, so it is produced straight from the tree. Re-rendering the output reproduces the original geometry in both OpenSCAD and OpenRSCAD.
