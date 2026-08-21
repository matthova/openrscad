// Implicit slice count for a twisted extrude: OpenSCAD derives it from the
// twist, the profile's outermost radius, and $fa/$fs — not from the twist
// alone. A fixed "twist/15" rule reads 6.8% high here.
linear_extrude(height=10, twist=90) square(10);
