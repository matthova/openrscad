// `$fn` passed as a call argument must reach the children: the circle here is a
// 32-gon, not the 16-gon $fa/$fs would give. Silently resolving the child from
// the enclosing scope changes the mesh of every child of such a call.
linear_extrude(height=1, $fn=32) circle(5);
translate([20,0,0], $fn=64) sphere(5);
