// The `d=` spellings, and the precedence of the per-end diameters over `d`.
sphere(d = 8, $fn = 24);
translate([15,0,0]) cylinder(h = 6, d = 5, $fn = 24);
translate([30,0,0]) cylinder(h = 6, d1 = 2, d2 = 8, $fn = 24);
translate([45,0,0]) cylinder(h = 6, d = 9, d1 = 2, $fn = 24);   // d1 wins at the base
translate([60,0,0]) linear_extrude(2) circle(d = 7, $fn = 24);
