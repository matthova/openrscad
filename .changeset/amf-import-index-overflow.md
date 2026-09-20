---
"openrscad-release-root": patch
---

geom: importing a malformed AMF file no longer panics — triangle faces that reference an out-of-range or oversized vertex index are dropped instead of crashing the importer
