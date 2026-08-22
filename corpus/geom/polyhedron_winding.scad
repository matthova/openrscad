// OpenSCAD winds a face clockwise seen from outside. The same tetrahedron is
// written both ways below: the second block is inside-out. A lone inside-out
// solid exports with its normals reversed in both engines, but once it reaches
// the CSG kernel it is a positive solid, so it *adds* under union and *cuts*
// under difference rather than doing the reverse.
tetra = [[0,0,0],[2,0,0],[0,2,0],[0,0,2]];
out = [[0,1,2],[0,3,1],[1,3,2],[0,2,3]];   // as documented
inv = [[0,2,1],[0,1,3],[1,2,3],[0,3,2]];   // inside-out

polyhedron(tetra, out);                    // union of two disjoint solids
translate([6,0,0]) cube(3);

translate([0,10,0]) {
  polyhedron(tetra, inv);                  // same, but inside-out: still adds
  translate([6,0,0]) cube(3);
}

translate([0,20,0]) difference() { cube(3); translate([0.5,0.5,-1]) polyhedron(tetra, out); }
translate([0,30,0]) difference() { cube(3); translate([0.5,0.5,-1]) polyhedron(tetra, inv); }
