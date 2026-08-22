//! `.csg` export is a serialization of the evaluated tree, so the contract that
//! matters is a round trip: re-rendering the output must give the same geometry.
//!
//! Byte-identical text is deliberately not the test. OpenSCAD omits parameters
//! left at their defaults and the IR does not record whether a value was
//! written or defaulted, so writing every parameter explicitly — equally valid
//! input — is what we do. These cases were also checked against OpenSCAD
//! 2024.12.17 by re-rendering our output with it; that comparison needs the
//! binary, so it does not run here.

use openrscad_eval::{eval_program_with, export_csg, NullResolver};

/// Evaluate a source string and return the exported tree plus its volume.
fn render(source: &str) -> (String, f64) {
    let program = openrscad_syntax::parse(source).expect("parse");
    let out = eval_program_with(&program, &NullResolver, ".").expect("eval");
    let mesh = openrscad_geom::render(&out.node).expect("render");
    (export_csg(&out.node), mesh.volume())
}

/// A round trip is lossy *by construction*: like upstream, the format writes six
/// significant digits, so a rotation matrix comes back slightly rounded. The
/// tolerance below reflects that text precision, and is still an order of
/// magnitude tighter than the geometry oracle's 0.1%.
const ROUND_TRIP_TOL: f64 = 1e-4;

#[track_caller]
fn round_trips(source: &str) {
    let (csg, volume) = render(source);
    let (again, volume2) = render(&csg);
    assert!(
        (volume - volume2).abs() <= volume.abs() * ROUND_TRIP_TOL + 1e-9,
        "volume changed on re-render: {volume} -> {volume2}\n--- first pass ---\n{csg}\n\
         --- second pass ---\n{again}"
    );
    // A second export must be a fixed point: if it is not, the writer and the
    // reader disagree about something the first pass happened to survive.
    assert_eq!(csg, again, "export is not idempotent");
}

#[test]
fn primitives_and_booleans_round_trip() {
    round_trips("cube(3);");
    round_trips("sphere(5, $fn=16);");
    round_trips("cylinder(h=4, r1=1, r2=2, $fn=12);");
    round_trips("polyhedron([[0,0,0],[2,0,0],[0,2,0],[0,0,2]],[[0,2,1],[0,1,3],[1,2,3],[0,3,2]]);");
    round_trips("difference(){ cube(4); translate([1,1,-1]) cylinder(h=6,r=1,$fn=12); }");
    round_trips("intersection(){ cube(2,center=true); sphere(1.3,$fn=16); }");
    round_trips("union(){ cube(2); rotate([0,45,30]) cube([3,1,1]); }");
    round_trips("hull(){ sphere(1,$fn=12); translate([5,0,0]) sphere(1,$fn=12); }");
    round_trips("minkowski(){ cube(3); sphere(0.5,$fn=8); }");
}

#[test]
fn transforms_round_trip_through_multmatrix() {
    // `.csg` has no translate/rotate/scale/mirror — each lowers to a matrix, so
    // the reader has to reconstruct the same placement from it.
    round_trips("scale([2,1,0.5]) mirror([1,0,0]) cube(2);");
    round_trips("rotate([15,30,45]) cube([3,1,2]);");
    round_trips("multmatrix([[1,0,0.5,1],[0,1,0,0],[0,0,1,0],[0,0,0,1]]) cube(2);");
    round_trips("resize([6,0,0], auto=true) cube(2);");
}

#[test]
fn two_dimensional_and_extrusions_round_trip() {
    round_trips("linear_extrude(height=5, twist=45, $fn=16) square([2,3], center=true);");
    round_trips("rotate_extrude(angle=200, $fn=16) translate([3,0]) square([1,2]);");
    round_trips("linear_extrude(2) polygon([[0,0],[4,0],[4,3],[1,3]]);");
    round_trips("linear_extrude(1) offset(r=1, $fn=16) square(4);");
    round_trips("linear_extrude(1) projection(cut=true) translate([0,0,-1]) sphere(3,$fn=16);");
}

#[test]
fn display_attributes_survive_the_round_trip() {
    // `%` background drops out of the rendered result, so a round trip that
    // mishandled it would change the volume rather than just the text.
    round_trips("color(\"red\") cube(2); #cube(1); translate([5,0,0]) cube(1);");
    round_trips("%cube(9); cube(2);");
}

#[test]
fn an_omitted_slice_count_stays_derived() {
    // Freezing a resolved `slices=` into the output would pin the tessellation
    // to whatever the first render chose.
    let (csg, _) = render("linear_extrude(height=5, twist=30) square(2);");
    assert!(!csg.contains("slices"), "{csg}");
    // An explicit one must survive, though.
    let (csg, _) = render("linear_extrude(height=5, twist=30, slices=7) square(2);");
    assert!(csg.contains("slices = 7"), "{csg}");
}
