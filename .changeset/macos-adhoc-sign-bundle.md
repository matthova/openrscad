---
"openrscad-release-root": patch
---

desktop: fix the macOS download reporting "OpenRSCAD is damaged and can't be opened". The .app bundle is now ad-hoc code-signed at build time (Apple Silicon and Intel), so Gatekeeper treats a downloaded copy as unverified rather than damaged — open it once, then click Open Anyway in System Settings → Privacy & Security. The release workflow now fails a macOS build whose bundle has no valid signature.
