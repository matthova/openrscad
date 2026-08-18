---
"openrscad-release-root": minor
---

Add native `exportShape3D` support for STL, OFF, OBJ, 3MF, AMF, and deterministic multipart GLB, plus `renderToGlb` for preview-semantics viewers. GLB preserves authored user-module hierarchy, source provenance, desktop-compatible materials, anonymous hexadecimal fallback parts, and optional owner-local feature lines. 3MF keeps complete authored geometry in independently named manifold objects with per-triangle materials and namespace-qualified provenance metadata. Export diagnostics now retain evaluation and geometry warnings/errors, and one production Wasm ABI ships every API without a feature-gated variant.
