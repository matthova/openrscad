// Twist combined with a non-uniform scale (A-G11). Each edge is refined by the
// peak stretch its direction reaches over the swept slices — not the twist-only
// length, which read 0.8% high in volume and split the segment budget the wrong
// way between edges. Oracle: OpenSCAD 2021.01+ reports volume 246.444, 1244 tris.
// oracle: tris
$fa=8;$fs=1.5;
linear_extrude(height=7,twist=200,scale=[0.4,1.6],center=true) square([8,5]);
