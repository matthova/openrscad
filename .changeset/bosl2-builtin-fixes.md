---
"openrscad-release-root": patch
---

fix three builtin behaviors that OpenSCAD gets right, closing six BOSL2 test blocks (505→511/513): `each` over a string now spreads its characters (`[each "ab"]` is `["a","b"]`); a range literal with a non-numeric bound or step is now `undef` (a nan/inf bound still makes a range, which equals itself); and `search` now matches a list-valued key against column 0 or a whole row.
