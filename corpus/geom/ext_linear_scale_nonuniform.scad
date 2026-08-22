// A non-uniform scale bends the extrusion walls the way a twist does, so
// OpenSCAD refines the profile and adds slices for it. This is volume-neutral —
// the walls stay ruled surfaces — so the tessellation is the only witness, and
// `tris` is pinned to make it part of the contract.
//
// One object only: several top-level objects would be unioned, and the kernel
// merges the coplanar wall facets back together (596 triangles collapse to 12),
// hiding exactly what this case exists to check.
//
// oracle: tris
linear_extrude(height=10, scale=[0.2,2]) square(10);
