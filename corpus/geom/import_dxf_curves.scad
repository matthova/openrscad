// DXF curve entities and the caller's fragment settings.
//
// `curves.dxf` holds three things: a polyline whose first segment carries a
// bulge, a full ELLIPSE, and a SPLINE. Only the ellipse contributes — OpenSCAD
// 2024.12 imports neither bulges nor splines, so expanding them here would
// disagree with the oracle rather than agree with it. `arcs.dxf` is a quarter
// disc built from two lines and an ARC.
//
// $fn/$fa/$fs at the call site tessellate all of these; before, arcs were
// always cut at the default resolution however the caller set them.
//
// oracle: tris
$fn = 48;
linear_extrude(1) import("curves.dxf");
translate([0,30,0]) linear_extrude(1) import("arcs.dxf");
