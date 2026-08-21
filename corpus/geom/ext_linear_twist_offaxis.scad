// A twisted profile that does not straddle the Z axis: the walls lean
// differently edge by edge, so a single diagonal choice for the whole contour is
// 1.3% out even though every vertex is in the right place.
//
// oracle: tris
linear_extrude(height=10, twist=90, slices=4) translate([20,0]) square(10);
