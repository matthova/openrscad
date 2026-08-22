// `layer=` keeps only that DXF layer, and `origin`/`scale` place the outline as
// (point - origin) * scale. Both were accepted and silently ignored before, so
// this file imported the whole drawing three times over.
//
// oracle: tris
linear_extrude(1) import("layers.dxf", layer="A");
translate([20,0,0]) linear_extrude(1) import("layers.dxf", layer="B");
translate([40,0,0]) linear_extrude(1) import("layers.dxf", layer="A", origin=[1,1], scale=2);
