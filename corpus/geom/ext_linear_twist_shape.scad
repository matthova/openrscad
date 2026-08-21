// Unequal edge lengths get unequal shares of the refinement budget, and a
// long/thin profile exercises the per-edge minimum.
$fa = 6; $fs = 1;
linear_extrude(height=10, twist=180) square([10,3]);
