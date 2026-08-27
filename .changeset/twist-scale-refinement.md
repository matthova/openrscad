---
"openrscad-release-root": patch
---

fix `linear_extrude` combining twist with a non-uniform scale — the last silent geometry difference. Each edge is now refined by the peak stretch its direction reaches across the swept slices, and the layer sweep rotates then scales in the fixed frame, so the headline case is exact (was 0.8% high in volume).
