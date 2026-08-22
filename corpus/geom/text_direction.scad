// Right-to-left and vertical runs.
//
// RTL is not "reverse the codepoints": Arabic glyphs join and change form, so
// the shaped run is what has to match. Vertical text is not a baseline run
// either — each glyph is centred in a slot whose height is the OS/2
// typographic ascent-to-descent span, and centred horizontally on the column.
linear_extrude(1) text("مرحبا", size=10, direction="rtl", script="arabic");
translate([0,-16,0]) linear_extrude(1) text("abc", size=10, direction="ttb");
translate([14,-16,0]) linear_extrude(1) text("Hg", size=10, direction="ttb");
