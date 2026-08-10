// A U-prism whose cap is not star-shaped from its first vertex. The cube lies
// wholly in its open notch, so the intersection is empty. A fan-triangulated
// cap overlaps that notch and makes the input non-manifold; the boolean then
// degrades to a visibly non-empty fallback instead.
// oracle: tris
intersection() {
    polyhedron(
        points=[
        [0,0,0], [4,0,0], [4,4,0], [3,4,0], [3,1,0], [1,1,0], [1,4,0], [0,4,0],
        [0,0,2], [4,0,2], [4,4,2], [3,4,2], [3,1,2], [1,1,2], [1,4,2], [0,4,2]
        ],
        faces=[
        [0,1,2,3,4,5,6,7], [15,14,13,12,11,10,9,8],
        [0,8,9,1], [1,9,10,2], [2,10,11,3], [3,11,12,4],
        [4,12,13,5], [5,13,14,6], [6,14,15,7], [7,15,8,0]
        ]
    );
    translate([1.2,1.2,-1]) cube([1.6,2.6,4]);
}
