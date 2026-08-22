// With no physical width/height on the document, user units are inches at
// `dpi` — 72 by default. A document that does give a size ignores `dpi`.
//
// oracle: tris
linear_extrude(1) import("nodim.svg");
translate([40,0,0]) linear_extrude(1) import("nodim.svg", dpi=96);
