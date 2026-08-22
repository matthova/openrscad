// `layer=` selects an Inkscape layer group by its label; `id=` selects any
// element by id, group or shape.
//
// oracle: tris
linear_extrude(1) import("layers.svg", layer="LayerA");
translate([10,0,0]) linear_extrude(1) import("layers.svg", layer="LayerB");
translate([20,0,0]) linear_extrude(1) import("layers.svg", id="loose");
