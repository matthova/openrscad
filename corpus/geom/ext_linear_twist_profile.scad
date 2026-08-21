// The 2D profile is re-tessellated before a twisted sweep, independently of the
// slice count: with slices pinned, the mesh still changes with $fa/$fs. This is
// the dominant error term if a renderer only fixes the slice rule.
linear_extrude(height=10, twist=90, slices=3, $fa=3, $fs=0.5) square(10);
