// Shaped text: kerning and ligatures come from the shaper, not from summing
// per-codepoint advances. "AV" is a kern pair (1mm narrower than the naive
// sum), "ffl" is a ligature, and a whole word accumulates both.
linear_extrude(1) text("AV", size=10, font="Liberation Sans");
translate([0,-14,0]) linear_extrude(1) text("ffl", size=10, font="Liberation Serif");
translate([0,-28,0]) linear_extrude(1) text("Tomato", size=10, font="Liberation Sans");
