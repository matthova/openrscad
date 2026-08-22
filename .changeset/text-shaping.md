---
"openrscad-release-root": minor
---

`text()` is now shaped with `rustybuzz`, the Rust port of the same HarfBuzz shaper OpenSCAD uses, instead of summing per-glyph advances. Kerning pairs and ligatures come out right — `"AV"` was a millimetre too wide and `"ffl"` missed its ligature — and joining scripts render at all, where Arabic previously produced nothing. `direction=` supports `ltr`/`rtl`/`ttb`/`btt` with `script=`/`language=` passed through, vertical runs centre each glyph in its slot as upstream does, and `valign` now aligns the ink box rather than the font ascender.
