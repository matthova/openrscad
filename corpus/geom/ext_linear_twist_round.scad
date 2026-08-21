// A 32-gon twisted by exactly one vertex step per slice is still a prism. Only
// one of the two wall-quad diagonals reproduces that; the other cuts ~1.3% off.
$fn = 32;
linear_extrude(height=10, twist=90) circle(5);
