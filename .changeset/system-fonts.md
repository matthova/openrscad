---
"openrscad-release-root": minor
---

fonts: `text(font="…")` can now use your system fonts, not just the bundled Liberation family. Native (CLI, desktop, and the LSP) reads installed fonts automatically. Both apps add a "System fonts" toggle (Display ▾): the desktop app lists your installed fonts in autocomplete (on by default — no permission needed); the web playground (Chromium browsers) grants access to your local fonts via the Local Font Access API. The `font=` autocomplete lists every available font accordingly (bundled-only where system fonts aren't enabled), and now previews the highlighted font — a pangram sample rendered in that actual typeface — as you scroll the list.
