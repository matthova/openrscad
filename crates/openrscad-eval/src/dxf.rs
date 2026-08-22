//! Just enough DXF to answer the deprecated `dxf_dim()` and `dxf_cross()`
//! functions, which read a measurement or an intersection point out of a
//! drawing rather than importing geometry.
//!
//! The geometry crate has a fuller DXF reader, but `openrscad-eval` does not
//! depend on it — these two functions need a handful of group codes, not a
//! polygon importer, so the scan lives here instead of inverting that layering.
//!
//! Behaviour was measured against OpenSCAD 2024.12.17 using authored fixtures;
//! see `corpus/echo/dxf_query.scad`.

/// A DXF entity: its type name and its `(group code, value)` pairs, in order.
type Entity = (String, Vec<(i32, String)>);

/// Scan the `ENTITIES` section into `(type, groups)` pairs. DXF is a flat
/// stream of code/value line pairs; a group code of 0 starts a new entity.
fn entities(text: &str) -> Vec<Entity> {
    let mut lines = text.lines().map(str::trim);
    let mut out: Vec<Entity> = Vec::new();
    let mut in_entities = false;
    let mut current: Option<Entity> = None;
    while let (Some(code), Some(value)) = (lines.next(), lines.next()) {
        let Ok(code) = code.parse::<i32>() else {
            continue;
        };
        if code == 0 {
            if let Some(e) = current.take() {
                out.push(e);
            }
            match value {
                "SECTION" => {}
                "ENDSEC" | "EOF" => in_entities = false,
                ty if in_entities => current = Some((ty.to_string(), Vec::new())),
                _ => {}
            }
        } else if code == 2 && value == "ENTITIES" {
            in_entities = true;
        } else if let Some((_, groups)) = current.as_mut() {
            groups.push((code, value.to_string()));
        }
    }
    if let Some(e) = current {
        out.push(e);
    }
    out
}

fn group(groups: &[(i32, String)], code: i32) -> Option<&str> {
    groups
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, v)| v.as_str())
}

fn num(groups: &[(i32, String)], code: i32) -> Option<f64> {
    group(groups, code).and_then(|v| v.parse().ok())
}

fn coord(groups: &[(i32, String)], x: i32) -> [f64; 2] {
    [
        num(groups, x).unwrap_or(0.0),
        num(groups, x + 10).unwrap_or(0.0),
    ]
}

/// An empty layer selector matches every layer, as upstream.
fn on_layer(groups: &[(i32, String)], layer: &str) -> bool {
    layer.is_empty() || group(groups, 8) == Some(layer)
}

/// The measurement of the named `DIMENSION`, or `None` if there is no match.
///
/// An empty `name` takes the first dimension on the layer. The name is matched
/// against the dimension *text* (group 1), not the block name, and the stored
/// measurement (group 42) is ignored — OpenSCAD recomputes it from the
/// definition points, so a drawing whose 42 disagrees still reports the
/// geometry.
pub fn dim(text: &str, layer: &str, name: &str) -> Option<f64> {
    for (ty, g) in entities(text) {
        if ty != "DIMENSION" || !on_layer(&g, layer) {
            continue;
        }
        if !name.is_empty() && group(&g, 1) != Some(name) {
            continue;
        }
        // The low three bits select the kind; the rest are presentation flags.
        let kind = num(&g, 70).unwrap_or(0.0) as i64 & 7;
        let (p13, p14) = (coord(&g, 13), coord(&g, 14));
        let (dx, dy) = (p14[0] - p13[0], p14[1] - p13[1]);
        return Some(match kind {
            // Linear, measured along the group-50 rotation: horizontal by
            // default, vertical at 90, and the projection onto any other angle.
            0 => {
                let rot = num(&g, 50).unwrap_or(0.0).to_radians();
                (dx * rot.cos() + dy * rot.sin()).abs()
            }
            // Aligned: the plain distance between the extension-line points.
            1 => dx.hypot(dy),
            // Radius and diameter both report the centre-to-chord distance.
            3 | 4 => {
                let (c, r) = (coord(&g, 10), coord(&g, 15));
                (r[0] - c[0]).hypot(r[1] - c[1])
            }
            _ => return None,
        });
    }
    None
}

/// The intersection of the first two non-parallel `LINE`s on the layer.
pub fn cross(text: &str, layer: &str) -> Option<[f64; 2]> {
    let lines: Vec<([f64; 2], [f64; 2])> = entities(text)
        .iter()
        .filter(|(ty, g)| ty == "LINE" && on_layer(g, layer))
        .map(|(_, g)| (coord(g, 10), coord(g, 11)))
        .collect();
    for i in 0..lines.len() {
        for j in i + 1..lines.len() {
            let (a0, a1) = lines[i];
            let (b0, b1) = lines[j];
            let (r, s) = (
                [a1[0] - a0[0], a1[1] - a0[1]],
                [b1[0] - b0[0], b1[1] - b0[1]],
            );
            let denom = r[0] * s[1] - r[1] * s[0];
            if denom.abs() < 1e-12 {
                continue; // parallel: no crossing point
            }
            let t = ((b0[0] - a0[0]) * s[1] - (b0[1] - a0[1]) * s[0]) / denom;
            return Some([a0[0] + r[0] * t, a0[1] + r[1] * t]);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a DXF from `(group code, value)` pairs, one entity per slice.
    fn build(entities: &[&[(i32, &str)]]) -> String {
        let mut out = String::from("0\nSECTION\n2\nENTITIES\n");
        for e in entities {
            for (code, value) in *e {
                out.push_str(&format!("{code}\n{value}\n"));
            }
        }
        out.push_str("0\nENDSEC\n0\nEOF\n");
        out
    }

    /// A linear (type 0) dimension from the origin to `(x, y)`, measured along
    /// `rot` degrees.
    fn linear(text: &str, rot: &str, x: &str, y: &str) -> Vec<(i32, String)> {
        vec![
            (0, "DIMENSION".into()),
            (8, "D".into()),
            (1, text.into()),
            (70, "0".into()),
            (13, "0".into()),
            (23, "0".into()),
            (14, x.into()),
            (24, y.into()),
            (50, rot.into()),
        ]
    }

    fn as_pairs(v: &[(i32, String)]) -> Vec<(i32, &str)> {
        v.iter().map(|(c, s)| (*c, s.as_str())).collect()
    }

    #[test]
    fn linear_dimensions_project_onto_the_rotation() {
        let (a, b, c) = (
            linear("rot0", "0", "6", "8"),
            linear("rot90", "90", "6", "8"),
            linear("rot45", "45", "6", "8"),
        );
        let text = build(&[&as_pairs(&a), &as_pairs(&b), &as_pairs(&c)]);
        assert_eq!(dim(&text, "", "rot0"), Some(6.0));
        assert_eq!(dim(&text, "", "rot90"), Some(8.0));
        assert!((dim(&text, "", "rot45").unwrap() - 9.899_494_9).abs() < 1e-6);
        // An empty name takes the first dimension; a wrong layer matches none.
        assert_eq!(dim(&text, "", ""), Some(6.0));
        assert_eq!(dim(&text, "OTHER", "rot0"), None);
    }

    #[test]
    fn aligned_and_radial_dimensions() {
        let aligned: Vec<(i32, &str)> = vec![
            (0, "DIMENSION"),
            (8, "D"),
            (1, "aligned"),
            (70, "1"),
            (13, "0"),
            (23, "0"),
            (14, "6"),
            (24, "8"),
        ];
        // Presentation bits above the low three must not change the kind.
        let flagged: Vec<(i32, &str)> = vec![
            (0, "DIMENSION"),
            (8, "D"),
            (1, "flagged"),
            (70, "128"),
            (13, "0"),
            (23, "0"),
            (14, "6"),
            (24, "8"),
        ];
        let radius: Vec<(i32, &str)> = vec![
            (0, "DIMENSION"),
            (8, "D"),
            (1, "radius"),
            (70, "4"),
            (10, "0"),
            (20, "0"),
            (15, "3"),
            (25, "4"),
        ];
        let text = build(&[&aligned, &flagged, &radius]);
        assert_eq!(dim(&text, "", "aligned"), Some(10.0));
        assert_eq!(dim(&text, "", "flagged"), Some(6.0));
        assert_eq!(dim(&text, "", "radius"), Some(5.0));
    }

    #[test]
    fn crossing_lines_yield_their_intersection() {
        let a: Vec<(i32, &str)> = vec![
            (0, "LINE"),
            (8, "L1"),
            (10, "0"),
            (20, "0"),
            (11, "10"),
            (21, "10"),
        ];
        let b: Vec<(i32, &str)> = vec![
            (0, "LINE"),
            (8, "L1"),
            (10, "0"),
            (20, "10"),
            (11, "10"),
            (21, "0"),
        ];
        let lone: Vec<(i32, &str)> = vec![
            (0, "LINE"),
            (8, "L2"),
            (10, "20"),
            (20, "0"),
            (11, "30"),
            (21, "0"),
        ];
        let text = build(&[&a, &b, &lone]);
        assert_eq!(cross(&text, "L1"), Some([5.0, 5.0]));
        // An empty selector matches any layer.
        assert_eq!(cross(&text, ""), Some([5.0, 5.0]));
        // A single line on the layer has nothing to cross.
        assert_eq!(cross(&text, "L2"), None);
    }
}
