---
"openrscad-release-root": patch
---

Recursion that exceeds the depth limit is now non-fatal, matching OpenSCAD's "Recursion detected calling function/module '…'" behavior. The offending call raises a contained error that aborts its enclosing CSG node, and at the program root the top-level traversal stops at the first such abort: geometry from statements *before* it still renders, while the offending statement and everything after it are dropped. Models that recurse forever on a corner case — e.g. the Ultimate Parametric Battery Organizer, whose helper recurses without bound on a single-slot row — now render exactly as they do in OpenSCAD (identical bounding box and volume), instead of aborting the whole render or leaving stray triangles behind.
