// Combined negative twist with non-uniform scale: the sweep rotates the other
// way, and the scale still applies in the fixed frame after the rotation.
// oracle: tris
$fa=6;$fs=1.2;
linear_extrude(height=9,twist=-160,scale=[1.8,0.5]) square([6,9]);
