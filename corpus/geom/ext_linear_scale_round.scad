// Non-uniform scale on a curved profile: no two wall quads lean alike, which
// makes this the sharpest check that the diagonal is chosen per quad rather than
// per contour.
//
// oracle: tris
$fn = 24;
linear_extrude(height=10, scale=[0.2,2]) circle(5);
