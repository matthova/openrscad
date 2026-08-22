// `convexity=` is an OpenCSG preview hint everywhere it appears: accepted, and
// with no effect on the rendered geometry. That is the claim under test — each
// pair below must come out identical.
polyhedron(points=[[0,0,0],[2,0,0],[0,2,0],[0,0,2]], faces=[[0,1,2],[0,3,1],[1,3,2],[0,2,3]], convexity=5);
translate([5,0,0]) linear_extrude(2, convexity=7) polygon([[0,0],[3,0],[3,3],[1,1]], convexity=3);
translate([12,0,0]) rotate_extrude(angle=120, convexity=4, $fn=16) translate([2,0]) square([1,2]);
translate([20,0,0]) resize([4,0,0], convexity=2) cube(2);
translate([28,0,0]) import("cube.stl", convexity=6);
