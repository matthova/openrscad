// Combined twist + non-uniform scale over a profile with a hole: both loops are
// refined by the same peak-stretch rule and swept rotate-then-scale.
// oracle: tris
$fn=48;
linear_extrude(height=8,twist=140,scale=[0.5,1.5]) difference(){ square([10,10],center=true); circle(3); }
