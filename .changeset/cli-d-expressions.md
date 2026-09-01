---
"openrscad-release-root": minor
---

`-D name=<expr>` now accepts arbitrary expressions (e.g. `-D 'm=[[1,2],[3,4]]'`, `-D 'r=sqrt(2)*2'`), matching OpenSCAD, via the new `openrscad_eval::eval_const_expr`
