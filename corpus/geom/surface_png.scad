// An 8-bit greyscale PNG is a heightfield too: each pixel's grey level is its
// height, so a 5x4 image gives the same lattice a 5x4 .dat would.
surface("heights.png");
translate([0, 10, 0]) surface("heights.png", center = true, invert = true);
