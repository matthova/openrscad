// `children()` with no argument, an index, a list of indices, and a range.
module all()    { children(); }
module one()    { children(1); }
module some()   { children([0, 2]); }
module ranged() { children([1:2]); }
all()    { cube(2); translate([3,0,0]) cube(2); }
translate([0,8,0])  one()    { cube(9); translate([3,0,0]) cube(2); }
translate([0,16,0]) some()   { cube(2); translate([3,0,0]) cube(9); translate([6,0,0]) cube(2); }
translate([0,24,0]) ranged() { cube(9); translate([3,0,0]) cube(2); translate([6,0,0]) cube(2); }
