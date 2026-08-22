// Element transforms, inherited down nested groups and composed with an
// element's own `transform=`. The importer used to walk tags flat, so every one
// of these landed unmoved at the origin.
//
// oracle: tris
linear_extrude(1) import("transforms.svg");
