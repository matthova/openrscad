// `segments=` overrides $fn/$fa/$fs for the profile and composes with an
// explicit slice count.
linear_extrude(height=10, twist=90, slices=4, segments=8, $fn=40) square(10);
