// `center` shifts the heightfield to straddle the origin and `invert` negates
// the sampled heights (the solid still runs down to the field's floor, so this
// is not merely a mirror). `convexity` is a preview hint with no mesh effect.
surface("heights.dat");
translate([0, 10, 0]) surface("heights.dat", center = true);
translate([0, 20, 0]) surface("heights.dat", invert = true);
translate([0, 30, 0]) surface(file = "heights.dat", center = true, invert = true, convexity = 3);
