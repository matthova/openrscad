// With $fn set, both the slice count and the profile refinement follow it:
// $fn slices per full revolution, and the outline is resampled to $fn segments.
$fn = 40;
linear_extrude(height=10, twist=90) square(10);
