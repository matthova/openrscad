// A non-positive dimension yields no geometry, as upstream. Every primitive
// below contributes nothing, so only the valid cube and cone remain. Emitting
// degenerate geometry instead is not merely zero-volume: those triangles are
// non-manifold and take the enclosing boolean down with them.
cube([-2,3,4]);
cube(-5);
cube(0);
cube([0,3,4]);
sphere(-5);
sphere(0);
cylinder(h=-5, r=2);
cylinder(h=0, r=2);
cylinder(h=5, r=-2);
cylinder(h=5, r=0);
cylinder(h=5, r1=-1, r2=3);
cylinder(h=5, r1=0, r2=0);
linear_extrude(height=-5) square(2);
linear_extrude(height=0) square(2);
linear_extrude(1) square([-2,3]);
linear_extrude(1) square(0);
linear_extrude(1) circle(-5);

// Still valid: one radius may be zero, which is a cone.
translate([30,0,0]) cylinder(h=5, r1=0, r2=3);
// And a degenerate operand must not break a boolean.
translate([50,0,0]) difference() { cube(10); cube(0); }

// NaN and infinity are invalid dimensions for primitives...
translate([70,0,0]) { cube(0/0); cube(1/0); sphere(0/0); cylinder(h=1/0,r=2); }
// ...but for linear_extrude a non-finite height counts as *unset*, so it falls
// back to the default of 100 rather than extruding nothing.
translate([90,0,0]) linear_extrude(height=1/0) square(2);
