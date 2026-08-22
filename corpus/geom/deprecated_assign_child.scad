// The retained deprecated 2021.01 forms, driving real geometry.
//
// `assign()` is not `let()`: every right-hand side is evaluated in the
// enclosing scope and the bindings take effect together, so `t` below is the
// *outer* s (3) plus 9 = 12, not 13.
s = 3;
assign(s = 4, t = s + 9) { cube(s); translate([10,0,0]) cube([t,2,2]); }

// Bare `child()` is the first child alone, where bare `children()` is all of
// them; `child(i)` selects one.
module first_only() { child(); }
module second_one() { child(1); }
module all_of_them() { children(); }
translate([0,10,0]) first_only() { cube(2); cube(9); }
translate([0,20,0]) second_one() { cube(9); cube(2); }
translate([0,30,0]) all_of_them() { cube(2); translate([5,0,0]) cube(3); }
