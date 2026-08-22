// `color`, `render`, `group` and `assert` are all transparent to the exported
// geometry: colour is a preview tint, `render` forces an exact sub-render,
// `group` is a plain container, and a passing `assert` just yields its children.
color("red", 0.5) cube(3);
translate([5,0,0]) color([0,1,0]) cube(3);
translate([10,0,0]) render(convexity = 3) difference() { cube(4); translate([1,1,-1]) cube(2); }
translate([16,0,0]) group() { cube(2); translate([3,0,0]) cube(2); }
translate([24,0,0]) assert(1 < 2, "must hold") cube(3);
translate([29,0,0]) assert(true) { cube(2); translate([3,0,0]) cube(2); }
