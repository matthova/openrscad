// `!` promotes its subtree to the whole model: only the sphere survives, the
// cube and everything else is dropped.
cube(50);
!translate([5,0,0]) sphere(6, $fn=24);
cylinder(h=30, r=4);
