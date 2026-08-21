// Negative twist mirrors the positive case, so its volume must match
// ext_linear_twist. A twisted wall quad is non-planar and its two diagonals
// enclose different volumes, so the split has to follow the twist direction.
linear_extrude(height=10, twist=-90) square(10);
