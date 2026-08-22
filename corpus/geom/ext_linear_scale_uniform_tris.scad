// The contrast to ext_linear_scale_nonuniform: a uniform scale keeps every wall
// planar, so nothing is refined and the frustum stays at 12 triangles however
// far the scale is from 1.
//
// oracle: tris
linear_extrude(height=10, scale=0.5) square(10);
