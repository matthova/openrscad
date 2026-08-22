//! Serialize an evaluated CSG tree as OpenSCAD's `.csg` format.
//!
//! `.csg` is the flattened program: every module call resolved, every
//! expression evaluated, every transform lowered to a `multmatrix`. It is valid
//! OpenSCAD, so the meaningful contract is a *round trip* — re-rendering the
//! output must give the same geometry — rather than byte-identical text.
//!
//! Byte identity is out of reach anyway: OpenSCAD omits parameters left at
//! their defaults, and the IR does not record whether a value was written or
//! defaulted. Writing every parameter explicitly is equally valid input.

use crate::value::format_number;
use openrscad_ir::{FragmentSpec, Node};

/// Serialize a tree as a `.csg` document.
pub fn export_csg(node: &Node) -> String {
    let mut out = String::new();
    write_node(node, 0, &mut out);
    out
}

fn num(v: f64) -> String {
    format_number(v)
}

fn vec_of(vals: &[f64]) -> String {
    let items: Vec<String> = vals.iter().map(|v| num(*v)).collect();
    format!("[{}]", items.join(", "))
}

fn boolean(b: bool) -> &'static str {
    if b {
        "true"
    } else {
        "false"
    }
}

/// `$fn`/`$fa`/`$fs`, which OpenSCAD writes first on every curved primitive.
fn frags(f: &FragmentSpec) -> String {
    format!(
        "$fn = {}, $fa = {}, $fs = {}",
        num(f.fn_),
        num(f.fa),
        num(f.fs)
    )
}

fn indent(depth: usize, out: &mut String) {
    for _ in 0..depth {
        out.push('\t');
    }
}

/// A leaf: `name(args);`
fn leaf(name: &str, args: String, depth: usize, out: &mut String) {
    indent(depth, out);
    out.push_str(&format!("{name}({args});\n"));
}

/// A parent: `name(args) { children }`. OpenSCAD emits the braces even for a
/// single child.
fn parent(name: &str, args: String, children: &[&Node], depth: usize, out: &mut String) {
    indent(depth, out);
    out.push_str(&format!("{name}({args}) {{\n"));
    for c in children {
        write_node(c, depth + 1, out);
    }
    indent(depth, out);
    out.push_str("}\n");
}

/// Lower a transform to the 4x4 matrix OpenSCAD writes, since `.csg` has no
/// `translate`/`rotate`/`scale` — every transform arrives as a `multmatrix`.
fn multmatrix(m: [[f64; 4]; 4], child: &Node, depth: usize, out: &mut String) {
    let rows: Vec<String> = m.iter().map(|r| vec_of(r)).collect();
    parent(
        "multmatrix",
        format!("[{}]", rows.join(", ")),
        &[child],
        depth,
        out,
    );
}

fn translation(v: [f64; 3]) -> [[f64; 4]; 4] {
    [
        [1.0, 0.0, 0.0, v[0]],
        [0.0, 1.0, 0.0, v[1]],
        [0.0, 0.0, 1.0, v[2]],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn scaling(v: [f64; 3]) -> [[f64; 4]; 4] {
    [
        [v[0], 0.0, 0.0, 0.0],
        [0.0, v[1], 0.0, 0.0],
        [0.0, 0.0, v[2], 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// X then Y then Z, the OpenSCAD `rotate([x,y,z])` convention.
fn rotation(deg: [f64; 3]) -> [[f64; 4]; 4] {
    let [rx, ry, rz] = [
        deg[0].to_radians(),
        deg[1].to_radians(),
        deg[2].to_radians(),
    ];
    let (sx, cx) = rx.sin_cos();
    let (sy, cy) = ry.sin_cos();
    let (sz, cz) = rz.sin_cos();
    // Rz * Ry * Rx
    [
        [cz * cy, cz * sy * sx - sz * cx, cz * sy * cx + sz * sx, 0.0],
        [sz * cy, sz * sy * sx + cz * cx, sz * sy * cx - cz * sx, 0.0],
        [-sy, cy * sx, cy * cx, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// Householder reflection across the plane through the origin with normal `v`.
fn mirroring(v: [f64; 3]) -> [[f64; 4]; 4] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-12 {
        return translation([0.0, 0.0, 0.0]);
    }
    let n = [v[0] / len, v[1] / len, v[2] / len];
    let mut m = [[0.0; 4]; 4];
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] = if i == j { 1.0 } else { 0.0 } - 2.0 * n[i] * n[j];
        }
    }
    m[3][3] = 1.0;
    m
}

fn write_node(node: &Node, depth: usize, out: &mut String) {
    match node {
        // An empty subtree has no spelling; OpenSCAD writes nothing for it.
        Node::Empty => {}
        Node::Group(children) => {
            let refs: Vec<&Node> = children.iter().collect();
            parent("group", String::new(), &refs, depth, out);
        }

        Node::Cube { size, center } => leaf(
            "cube",
            format!("size = {}, center = {}", vec_of(size), boolean(*center)),
            depth,
            out,
        ),
        Node::Sphere { r, frags: f } => leaf(
            "sphere",
            format!("{}, r = {}", frags(f), num(*r)),
            depth,
            out,
        ),
        Node::Cylinder {
            h,
            r1,
            r2,
            center,
            frags: f,
        } => leaf(
            "cylinder",
            format!(
                "{}, h = {}, r1 = {}, r2 = {}, center = {}",
                frags(f),
                num(*h),
                num(*r1),
                num(*r2),
                boolean(*center)
            ),
            depth,
            out,
        ),
        Node::Polyhedron { points, faces } => {
            let pts: Vec<String> = points.iter().map(|p| vec_of(p)).collect();
            let fs: Vec<String> = faces
                .iter()
                .map(|f| {
                    let idx: Vec<String> = f.iter().map(|i| i.to_string()).collect();
                    format!("[{}]", idx.join(", "))
                })
                .collect();
            leaf(
                "polyhedron",
                format!(
                    "points = [{}], faces = [{}], convexity = 1",
                    pts.join(", "),
                    fs.join(", ")
                ),
                depth,
                out,
            )
        }

        Node::Square { size, center } => leaf(
            "square",
            format!("size = {}, center = {}", vec_of(size), boolean(*center)),
            depth,
            out,
        ),
        Node::Circle { r, frags: f } => leaf(
            "circle",
            format!("{}, r = {}", frags(f), num(*r)),
            depth,
            out,
        ),
        Node::Polygon { points, paths } => {
            let pts: Vec<String> = points.iter().map(|p| vec_of(p)).collect();
            let paths = match paths {
                None => "undef".to_string(),
                Some(ps) => {
                    let items: Vec<String> = ps
                        .iter()
                        .map(|p| {
                            let idx: Vec<String> = p.iter().map(|i| i.to_string()).collect();
                            format!("[{}]", idx.join(", "))
                        })
                        .collect();
                    format!("[{}]", items.join(", "))
                }
            };
            leaf(
                "polygon",
                format!(
                    "points = [{}], paths = {paths}, convexity = 1",
                    pts.join(", ")
                ),
                depth,
                out,
            )
        }

        Node::LinearExtrude {
            height,
            center,
            twist,
            scale,
            slices,
            segments,
            frags: f,
            child,
        } => {
            let mut args = format!(
                "height = {}, center = {}, twist = {}, scale = {}",
                num(*height),
                boolean(*center),
                num(*twist),
                vec_of(scale)
            );
            // Only write a slice or segment count that was actually requested;
            // an omitted one is derived from the profile at render time, and
            // writing a resolved number here would freeze it.
            if let Some(s) = slices {
                args.push_str(&format!(", slices = {s}"));
            }
            if *segments > 0 {
                args.push_str(&format!(", segments = {segments}"));
            }
            args.push_str(&format!(", {}", frags(f)));
            parent("linear_extrude", args, &[child], depth, out)
        }
        Node::RotateExtrude {
            angle,
            frags: f,
            child,
        } => parent(
            "rotate_extrude",
            format!("angle = {}, {}", num(*angle), frags(f)),
            &[child],
            depth,
            out,
        ),
        Node::Offset {
            r,
            delta,
            chamfer,
            frags: f,
            child,
        } => {
            let args = if *r != 0.0 {
                format!("r = {}, {}", num(*r), frags(f))
            } else {
                format!(
                    "delta = {}, chamfer = {}, {}",
                    num(*delta),
                    boolean(*chamfer),
                    frags(f)
                )
            };
            parent("offset", args, &[child], depth, out)
        }

        Node::Translate { v, child } => multmatrix(translation(*v), child, depth, out),
        Node::Rotate { deg, child } => multmatrix(rotation(*deg), child, depth, out),
        Node::Scale { v, child } => multmatrix(scaling(*v), child, depth, out),
        Node::Mirror { v, child } => multmatrix(mirroring(*v), child, depth, out),
        Node::MultMatrix { m, child } => multmatrix(*m, child, depth, out),
        Node::Resize { new, auto, child } => parent(
            "resize",
            format!(
                "newsize = {}, auto = [{}, {}, {}], convexity = 0",
                vec_of(new),
                boolean(auto[0]),
                boolean(auto[1]),
                boolean(auto[2])
            ),
            &[child],
            depth,
            out,
        ),

        Node::Union(children)
        | Node::Difference(children)
        | Node::Intersection(children)
        | Node::Hull(children)
        | Node::Minkowski(children) => {
            let (name, args) = match node {
                Node::Union(_) => ("union", String::new()),
                Node::Difference(_) => ("difference", String::new()),
                Node::Intersection(_) => ("intersection", String::new()),
                Node::Hull(_) => ("hull", String::new()),
                _ => ("minkowski", "convexity = 0".to_string()),
            };
            let refs: Vec<&Node> = children.iter().collect();
            parent(name, args, &refs, depth, out)
        }

        Node::Projection { cut, child } => parent(
            "projection",
            format!("cut = {}, convexity = 0", boolean(*cut)),
            &[child],
            depth,
            out,
        ),

        // An import cannot be inlined — `.csg` references the file, and the
        // bytes are all we kept, so emit a comment rather than a bogus path.
        Node::Import { format, .. } => {
            indent(depth, out);
            out.push_str(&format!(
                "// import({format}) omitted: .csg references a file\n"
            ));
        }

        Node::Color { rgba, child } => parent(
            "color",
            vec_of(&[
                rgba[0] as f64,
                rgba[1] as f64,
                rgba[2] as f64,
                rgba[3] as f64,
            ]),
            &[child],
            depth,
            out,
        ),
        // `#` and `%` are modifier characters, not calls; `%` also drops out of
        // the rendered result, and `.csg` keeps that distinction.
        Node::Highlight(child) => {
            let mut inner = String::new();
            write_node(child, depth, &mut inner);
            out.push_str(&prefix_first(&inner, '#'));
        }
        Node::Background(child) => {
            let mut inner = String::new();
            write_node(child, depth, &mut inner);
            out.push_str(&prefix_first(&inner, '%'));
        }
        // Provenance is our own bookkeeping and has no OpenSCAD spelling.
        Node::Provenance { child, .. } => write_node(child, depth, out),
    }
}

/// Put a modifier character immediately before the first non-tab character.
fn prefix_first(text: &str, ch: char) -> String {
    match text.find(|c: char| c != '\t') {
        Some(i) => format!("{}{ch}{}", &text[..i], &text[i..]),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_carry_their_parameters() {
        let cube = Node::Cube {
            size: [1.0, 2.0, 3.0],
            center: true,
        };
        assert_eq!(
            export_csg(&cube),
            "cube(size = [1, 2, 3], center = true);\n"
        );
        let sphere = Node::Sphere {
            r: 1.3,
            frags: FragmentSpec {
                fn_: 8.0,
                fa: 12.0,
                fs: 2.0,
            },
        };
        assert_eq!(
            export_csg(&sphere),
            "sphere($fn = 8, $fa = 12, $fs = 2, r = 1.3);\n"
        );
    }

    #[test]
    fn transforms_lower_to_multmatrix() {
        let node = Node::Translate {
            v: [1.0, 2.0, -1.0],
            child: Box::new(Node::Cube {
                size: [1.0, 1.0, 1.0],
                center: false,
            }),
        };
        assert_eq!(
            export_csg(&node),
            "multmatrix([[1, 0, 0, 1], [0, 1, 0, 2], [0, 0, 1, -1], [0, 0, 0, 1]]) {\n\
             \tcube(size = [1, 1, 1], center = false);\n}\n"
        );
    }

    #[test]
    fn modifiers_prefix_the_child() {
        let node = Node::Background(Box::new(Node::Cube {
            size: [3.0, 3.0, 3.0],
            center: false,
        }));
        assert_eq!(
            export_csg(&node),
            "%cube(size = [3, 3, 3], center = false);\n"
        );
    }

    #[test]
    fn an_omitted_slice_count_is_not_frozen_into_the_output() {
        // Writing a resolved count would pin the tessellation, so an omitted
        // `slices=` must stay omitted for the reader to derive again.
        let node = Node::LinearExtrude {
            height: 5.0,
            center: false,
            twist: 30.0,
            scale: [1.0, 1.0],
            slices: None,
            segments: 0,
            frags: FragmentSpec::default(),
            child: Box::new(Node::Square {
                size: [2.0, 2.0],
                center: false,
            }),
        };
        let text = export_csg(&node);
        assert!(!text.contains("slices"), "{text}");
        assert!(!text.contains("segments"), "{text}");
        assert!(text.contains("twist = 30"), "{text}");
    }
}
