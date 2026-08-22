// `<use>` instantiates another element, offset by its own x/y and placed by the
// transform in effect. Selecting a `display:none` element *by name* renders it
// anyway — that is what upstream does.
//
// oracle: tris
linear_extrude(1) import("structure.svg");
translate([20,0,0]) linear_extrude(1) import("structure.svg", id="gone1");
translate([30,0,0]) linear_extrude(1) import("structure.svg", id="inhidden");
