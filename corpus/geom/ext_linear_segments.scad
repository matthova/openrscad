// `segments=` subdivides the profile outline even with no twist at all — the
// $fn/$fa/$fs refinement only kicks in when twisting, but an explicit
// `segments=` is always honoured.
linear_extrude(height=10, segments=8) square(10);
