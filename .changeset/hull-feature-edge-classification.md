---
"openrscad-release-root": patch
---

`--edges` now draws the real outline of hulled geometry. A `hull()` of two cylinders used to file each operand's flat caps and its curved wall under one surface, so the rim around a rounded stroke end went undrawn while straight chords cut across the caps and seams ran down the smooth side walls — a letter built from hulled cylinder pairs lost most of its top outline. Hull faces are now cut apart at their creases, and the boundary between two surfaces is judged per connected run rather than per surface pair, so a boundary that is smooth in one place and creased in another is drawn only where it creases. Two visible consequences: hulling a coarse-`$fn` primitive now shows the facet creases sharper than 30°, and a genuinely sharp corner that a chain of smooth neighbours used to suppress — a reflex corner of an `offset` or a glyph — is drawn again.
