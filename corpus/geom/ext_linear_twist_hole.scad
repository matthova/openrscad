// A twisted profile with a hole. The hole's wall quads lean the opposite way to
// the outer's, because its indices run the other way round — and each quad is
// split along its shorter diagonal, so getting this wrong is a 0.6% volume error
// with an otherwise identical mesh.
//
// oracle: tris
linear_extrude(height=10, twist=90) difference() { square(10); translate([3,3]) square(4); }
