// `valign` aligns the *ink* box, not the font's ascender/descender: the ink top
// of "aaa" lands at y=0 for valign="top" even though it has no ascenders, so a
// string of x-height letters sits differently from one with tall ones.
// `halign` uses the advance width instead, which is why the two differ.
linear_extrude(1) text("hello", size=10, valign="top");
translate([0,-14,0]) linear_extrude(1) text("aaa", size=10, valign="top");
translate([0,-28,0]) linear_extrude(1) text("Hqp", size=10, valign="center");
translate([40,-28,0]) linear_extrude(1) text("Hqp", size=10, valign="bottom");
