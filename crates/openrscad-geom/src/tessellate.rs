//! Primitive tessellation, including the bit-exact fragment formula.
//!
//! Geometry compatibility with OpenSCAD hinges on the fragment count formula
//! and the exact vertex placement of curved primitives, so these are
//! reconstructed carefully from the documented behavior.

use crate::mesh::{cross, norm, sub, Mesh};
use openrscad_ir::FragmentSpec;
use std::f64::consts::PI;

/// OpenSCAD's `GRID_FINE` threshold below which a primitive collapses to the
/// minimum 3 fragments.
const GRID_FINE: f64 = 0.000_000_953_674_316_406_25;

/// Number of fragments in a full circle of radius `r`, given `$fn/$fa/$fs`.
pub fn fragments(r: f64, f: FragmentSpec) -> u32 {
    if r < GRID_FINE || f.fn_.is_nan() || f.fn_.is_infinite() {
        return 3;
    }
    if f.fn_ > 0.0 {
        return if f.fn_ >= 3.0 { f.fn_ as u32 } else { 3 };
    }
    let n = (360.0 / f.fa).min(r * 2.0 * PI / f.fs).max(5.0).ceil();
    n as u32
}

/// A point on a circle of `n` fragments, `i`-th fragment, radius `r`.
fn circle_point(r: f64, i: u32, n: u32) -> [f64; 2] {
    let phi = (2.0 * PI * i as f64) / n as f64;
    [r * phi.cos(), r * phi.sin()]
}

/// Build a mesh from explicit points and (possibly polygonal) faces.
///
/// OpenSCAD orders face vertices clockwise as seen from outside; our mesh
/// convention is counter-clockwise (outward normal by the right-hand rule), so
/// each emitted triangle reverses the face's winding. Convex faces retain the
/// historical fan (and therefore its stable triangle order); simple concave
/// faces are projected to their dominant 2D plane and triangulated with earcut.
/// Invalid indices and degenerate, self-intersecting, or materially non-planar
/// faces are skipped rather than leaving unsafe triangle references in the mesh.
pub fn polyhedron(points: &[[f64; 3]], faces: &[Vec<u32>]) -> Mesh {
    let verts = points.to_vec();
    let mut tris = Vec::new();
    for face in faces {
        tris.extend(triangulate_face(points, face));
    }
    Mesh { verts, tris }
}

/// Validate and triangulate one polygonal face. An empty result means the face
/// is malformed or geometrically degenerate and should be ignored.
fn triangulate_face(points: &[[f64; 3]], face: &[u32]) -> Vec<[u32; 3]> {
    // Clean only harmless adjacent/closing duplicates. A repeated vertex later
    // in the loop makes the polygon non-simple, so reject it below.
    let mut ids = Vec::with_capacity(face.len());
    for &id in face {
        if id as usize >= points.len() {
            return Vec::new();
        }
        if ids.last() != Some(&id) {
            ids.push(id);
        }
    }
    if ids.len() > 1 && ids.first() == ids.last() {
        ids.pop();
    }
    if ids.len() < 3 {
        return Vec::new();
    }
    for i in 0..ids.len() {
        if ids[..i].contains(&ids[i]) || !points[ids[i] as usize].iter().all(|x| x.is_finite()) {
            return Vec::new();
        }
    }

    let face_points: Vec<[f64; 3]> = ids.iter().map(|&i| points[i as usize]).collect();
    let mut lo = face_points[0];
    let mut hi = face_points[0];
    for p in &face_points[1..] {
        for axis in 0..3 {
            lo[axis] = lo[axis].min(p[axis]);
            hi[axis] = hi[axis].max(p[axis]);
        }
    }
    let extent = (0..3)
        .map(|axis| hi[axis] - lo[axis])
        .fold(0.0_f64, f64::max);
    if !extent.is_finite() || extent <= f64::EPSILON {
        return Vec::new();
    }
    let area_eps = extent * extent * 1e-12;

    // Newell's method gives a stable face normal for convex and concave planar
    // polygons alike. Its direction is the input face winding.
    let mut normal = [0.0; 3];
    for i in 0..face_points.len() {
        let p = face_points[i];
        let q = face_points[(i + 1) % face_points.len()];
        normal[0] += (p[1] - q[1]) * (p[2] + q[2]);
        normal[1] += (p[2] - q[2]) * (p[0] + q[0]);
        normal[2] += (p[0] - q[0]) * (p[1] + q[1]);
    }
    let normal_len = norm(normal);
    if !normal_len.is_finite() || normal_len <= area_eps {
        return Vec::new();
    }
    let unit = [
        normal[0] / normal_len,
        normal[1] / normal_len,
        normal[2] / normal_len,
    ];

    // A general polygon must describe one plane. Triangles are planar by
    // definition, while OpenSCAD commonly accepts warped quads (including the
    // project's curved lamp mesh) and their historical fan unambiguously
    // defines two planar triangles, so retain that compatible behavior.
    let plane_eps = extent * 1e-9 + 1e-12;
    let origin = face_points[0];
    let max_plane_distance = face_points
        .iter()
        .map(|&p| {
            let d = sub(p, origin);
            (d[0] * unit[0] + d[1] * unit[1] + d[2] * unit[2]).abs()
        })
        .fold(0.0_f64, f64::max);
    if face_points.len() > 4 && max_plane_distance > plane_eps {
        return Vec::new();
    }

    // Drop the normal's dominant coordinate. This maximizes projected area and
    // avoids numerical collapse for vertical or steeply slanted faces.
    let drop_axis = if normal[0].abs() >= normal[1].abs() && normal[0].abs() >= normal[2].abs() {
        0
    } else if normal[1].abs() >= normal[2].abs() {
        1
    } else {
        2
    };
    let projected: Vec<[f64; 2]> = face_points
        .iter()
        .map(|p| match drop_axis {
            0 => [p[1], p[2]],
            1 => [p[0], p[2]],
            _ => [p[0], p[1]],
        })
        .collect();
    let projected_eps = area_eps;
    let linear_eps = extent * 1e-12;
    let polygon_area2 = signed_area2(&projected).abs();
    if polygon_area2 <= projected_eps || !simple_polygon(&projected, linear_eps, projected_eps) {
        return Vec::new();
    }

    // Keep the exact historical fan for convex inputs. Besides being cheaper,
    // this preserves triangle order for existing primitives and goldens.
    if convex_polygon(&projected, projected_eps) {
        let mut out = Vec::with_capacity(ids.len() - 2);
        for k in 1..ids.len() - 1 {
            let tri = [ids[0], ids[k + 1], ids[k]];
            let a = points[tri[0] as usize];
            let b = points[tri[1] as usize];
            let c = points[tri[2] as usize];
            if norm(cross(sub(b, a), sub(c, a))) > area_eps {
                out.push(tri);
            }
        }
        return out;
    }

    let flat: Vec<f64> = projected.iter().flat_map(|p| [p[0], p[1]]).collect();
    let Ok(local_tris) = earcutr::earcut(&flat, &[], 2) else {
        return Vec::new();
    };
    if local_tris.len() % 3 != 0 {
        return Vec::new();
    }

    let mut covered_area2 = 0.0;
    let mut out = Vec::with_capacity(local_tris.len() / 3);
    for tri in local_tris.chunks_exact(3) {
        if tri.iter().any(|&i| i >= ids.len()) {
            return Vec::new();
        }
        let pa2 = projected[tri[0]];
        let pb2 = projected[tri[1]];
        let pc2 = projected[tri[2]];
        covered_area2 += cross2(pa2, pb2, pc2).abs();

        let a = ids[tri[0]];
        let mut b = ids[tri[1]];
        let mut c = ids[tri[2]];
        let pa = points[a as usize];
        let pb = points[b as usize];
        let pc = points[c as usize];
        let tri_normal = cross(sub(pb, pa), sub(pc, pa));
        if norm(tri_normal) <= area_eps {
            return Vec::new();
        }
        // Earcut may choose either 2D winding depending on the dropped axis.
        // OpenSCAD face winding is opposite ours, so make every triangle point
        // against the input face normal explicitly.
        if tri_normal[0] * normal[0] + tri_normal[1] * normal[1] + tri_normal[2] * normal[2] > 0.0 {
            std::mem::swap(&mut b, &mut c);
        }
        out.push([a, b, c]);
    }
    let coverage_eps = polygon_area2 * 1e-9 + projected_eps;
    if (covered_area2 - polygon_area2).abs() > coverage_eps {
        return Vec::new();
    }
    out
}

fn cross2(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn signed_area2(poly: &[[f64; 2]]) -> f64 {
    (0..poly.len())
        .map(|i| {
            let p = poly[i];
            let q = poly[(i + 1) % poly.len()];
            p[0] * q[1] - q[0] * p[1]
        })
        .sum()
}

fn convex_polygon(poly: &[[f64; 2]], eps: f64) -> bool {
    let mut sign = 0_i8;
    for i in 0..poly.len() {
        let turn = cross2(
            poly[i],
            poly[(i + 1) % poly.len()],
            poly[(i + 2) % poly.len()],
        );
        if turn.abs() <= eps {
            continue;
        }
        let next = if turn > 0.0 { 1 } else { -1 };
        if sign != 0 && sign != next {
            return false;
        }
        sign = next;
    }
    sign != 0
}

/// Reject self-intersections and backtracking edges before handing a face to
/// earcut. Adjacent edges may share their endpoint; no other touching is valid
/// for the simple faces accepted by `polyhedron()`.
fn simple_polygon(poly: &[[f64; 2]], linear_eps: f64, area_eps: f64) -> bool {
    let n = poly.len();
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        if (b[0] - a[0]).hypot(b[1] - a[1]) <= linear_eps {
            return false;
        }
        let c = poly[(i + 2) % n];
        if cross2(a, b, c).abs() <= area_eps {
            let ab = [b[0] - a[0], b[1] - a[1]];
            let bc = [c[0] - b[0], c[1] - b[1]];
            if ab[0] * bc[0] + ab[1] * bc[1] < 0.0 {
                return false;
            }
        }
        for j in i + 1..n {
            let next_i = (i + 1) % n;
            let next_j = (j + 1) % n;
            if i == j || next_i == j || next_j == i {
                continue;
            }
            if segments_intersect(a, b, poly[j], poly[next_j], linear_eps, area_eps) {
                return false;
            }
        }
    }
    true
}

fn segments_intersect(
    a: [f64; 2],
    b: [f64; 2],
    c: [f64; 2],
    d: [f64; 2],
    linear_eps: f64,
    area_eps: f64,
) -> bool {
    let ab_c = cross2(a, b, c);
    let ab_d = cross2(a, b, d);
    let cd_a = cross2(c, d, a);
    let cd_b = cross2(c, d, b);
    if ((ab_c > area_eps && ab_d < -area_eps) || (ab_c < -area_eps && ab_d > area_eps))
        && ((cd_a > area_eps && cd_b < -area_eps) || (cd_a < -area_eps && cd_b > area_eps))
    {
        return true;
    }
    let on_segment = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| {
        cross2(p, q, r).abs() <= area_eps
            && r[0] >= p[0].min(q[0]) - linear_eps
            && r[0] <= p[0].max(q[0]) + linear_eps
            && r[1] >= p[1].min(q[1]) - linear_eps
            && r[1] <= p[1].max(q[1]) + linear_eps
    };
    on_segment(a, b, c) || on_segment(a, b, d) || on_segment(c, d, a) || on_segment(c, d, b)
}

/// Axis-aligned box.
pub fn cube(size: [f64; 3], center: bool) -> Mesh {
    let (lo, hi) = if center {
        (
            [-size[0] / 2.0, -size[1] / 2.0, -size[2] / 2.0],
            [size[0] / 2.0, size[1] / 2.0, size[2] / 2.0],
        )
    } else {
        ([0.0, 0.0, 0.0], size)
    };
    let v = |x: bool, y: bool, z: bool| {
        [
            if x { hi[0] } else { lo[0] },
            if y { hi[1] } else { lo[1] },
            if z { hi[2] } else { lo[2] },
        ]
    };
    let verts = vec![
        v(false, false, false), // 0
        v(true, false, false),  // 1
        v(true, true, false),   // 2
        v(false, true, false),  // 3
        v(false, false, true),  // 4
        v(true, false, true),   // 5
        v(true, true, true),    // 6
        v(false, true, true),   // 7
    ];
    // Outward-facing (CCW) triangles.
    let tris = vec![
        [0, 3, 2],
        [0, 2, 1], // bottom (z-)
        [4, 5, 6],
        [4, 6, 7], // top (z+)
        [0, 1, 5],
        [0, 5, 4], // front (y-)
        [2, 3, 7],
        [2, 7, 6], // back (y+)
        [1, 2, 6],
        [1, 6, 5], // right (x+)
        [0, 4, 7],
        [0, 7, 3], // left (x-)
    ];
    let mut m = Mesh { verts, tris };
    m.ensure_outward();
    m
}

/// Sphere, tessellated with OpenSCAD's ring topology (flat poles).
pub fn sphere(r: f64, frags: FragmentSpec) -> Mesh {
    let n = fragments(r, frags).max(3);
    let num_rings = n.div_ceil(2);
    if num_rings == 0 || r <= 0.0 {
        return Mesh::new();
    }

    let mut verts: Vec<[f64; 3]> = Vec::with_capacity((num_rings * n) as usize);
    for i in 0..num_rings {
        let phi = (PI * (i as f64 + 0.5)) / num_rings as f64;
        let ring_r = r * phi.sin();
        let z = r * phi.cos();
        for j in 0..n {
            let p = circle_point(ring_r, j, n);
            verts.push([p[0], p[1], z]);
        }
    }

    let mut tris: Vec<[u32; 3]> = Vec::new();
    let idx = |ring: u32, j: u32| ring * n + (j % n);

    // side bands
    for i in 0..num_rings - 1 {
        for j in 0..n {
            let v00 = idx(i, j);
            let v01 = idx(i, j + 1);
            let v10 = idx(i + 1, j);
            let v11 = idx(i + 1, j + 1);
            tris.push([v00, v10, v11]);
            tris.push([v00, v11, v01]);
        }
    }

    // top cap (ring 0, near +z) — fan
    for j in 1..n - 1 {
        tris.push([idx(0, 0), idx(0, j), idx(0, j + 1)]);
    }
    // bottom cap (last ring, near -z) — fan (reversed)
    let last = num_rings - 1;
    for j in 1..n - 1 {
        tris.push([idx(last, 0), idx(last, j + 1), idx(last, j)]);
    }

    let mut m = Mesh { verts, tris };
    m.ensure_outward();
    m
}

/// Cylinder / cone / frustum along +Z.
pub fn cylinder(h: f64, r1: f64, r2: f64, center: bool, frags: FragmentSpec) -> Mesh {
    let n = fragments(r1.max(r2), frags).max(3);
    let (z0, z1) = if center {
        (-h / 2.0, h / 2.0)
    } else {
        (0.0, h)
    };

    let mut verts: Vec<[f64; 3]> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();

    let bottom_apex = r1 <= 0.0;
    let top_apex = r2 <= 0.0;

    // Both ends collapsed -> nothing.
    if bottom_apex && top_apex {
        return Mesh::new();
    }

    // Bottom vertices.
    let bottom_start = verts.len() as u32;
    if bottom_apex {
        verts.push([0.0, 0.0, z0]);
    } else {
        for j in 0..n {
            let p = circle_point(r1, j, n);
            verts.push([p[0], p[1], z0]);
        }
    }
    // Top vertices.
    let top_start = verts.len() as u32;
    if top_apex {
        verts.push([0.0, 0.0, z1]);
    } else {
        for j in 0..n {
            let p = circle_point(r2, j, n);
            verts.push([p[0], p[1], z1]);
        }
    }

    let b = |j: u32| bottom_start + (j % n);
    let t = |j: u32| top_start + (j % n);

    // Side walls.
    if bottom_apex {
        let apex = bottom_start;
        for j in 0..n {
            tris.push([apex, t(j + 1), t(j)]);
        }
    } else if top_apex {
        let apex = top_start;
        for j in 0..n {
            tris.push([b(j), b(j + 1), apex]);
        }
    } else {
        for j in 0..n {
            let b0 = b(j);
            let b1 = b(j + 1);
            let t0 = t(j);
            let t1 = t(j + 1);
            tris.push([b0, b1, t1]);
            tris.push([b0, t1, t0]);
        }
    }

    // Bottom cap (facing -Z): fan reversed.
    if !bottom_apex {
        for j in 1..n - 1 {
            tris.push([b(0), b(j + 1), b(j)]);
        }
    }
    // Top cap (facing +Z): fan.
    if !top_apex {
        for j in 1..n - 1 {
            tris.push([t(0), t(j), t(j + 1)]);
        }
    }

    let mut m = Mesh { verts, tris };
    m.ensure_outward();
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(fn_: f64) -> FragmentSpec {
        FragmentSpec {
            fn_,
            fa: 12.0,
            fs: 2.0,
        }
    }

    #[test]
    fn fragment_formula() {
        // $fn wins when >= 3
        assert_eq!(fragments(10.0, spec(6.0)), 6);
        assert_eq!(fragments(10.0, spec(1.0)), 3);
        // default $fa=12, $fs=2: for r=10 -> min(30, 31.4) -> 30
        assert_eq!(fragments(10.0, spec(0.0)), 30);
        // tiny radius collapses to 3
        assert_eq!(fragments(1e-9, spec(0.0)), 3);
        // minimum of 5
        assert_eq!(fragments(0.1, spec(0.0)), 5);
    }

    #[test]
    fn cube_volume_and_outward() {
        let m = cube([2.0, 3.0, 4.0], false);
        assert_eq!(m.tris.len(), 12);
        assert!((m.volume() - 24.0).abs() < 1e-9);
        assert!(m.signed_volume() > 0.0, "cube must be outward-facing");
    }

    #[test]
    fn sphere_outward_and_approx_volume() {
        let m = sphere(10.0, spec(64.0));
        assert!(m.signed_volume() > 0.0, "sphere must be outward-facing");
        let analytic = 4.0 / 3.0 * PI * 1000.0;
        // faceted sphere under-approximates; within ~2%.
        let rel = (m.volume() - analytic).abs() / analytic;
        assert!(rel < 0.02, "sphere volume off by {rel}");
    }

    #[test]
    fn cylinder_outward_and_volume() {
        let m = cylinder(10.0, 5.0, 5.0, false, spec(128.0));
        assert!(m.signed_volume() > 0.0);
        let analytic = PI * 25.0 * 10.0;
        let rel = (m.volume() - analytic).abs() / analytic;
        assert!(rel < 0.01, "cylinder volume off by {rel}");
    }

    #[test]
    fn polyhedron_convex_face_preserves_fan() {
        let pts = vec![[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.]];
        let faces = vec![vec![0u32, 1, 2, 3]]; // one quad -> 2 triangles
        let m = polyhedron(&pts, &faces);
        assert_eq!(m.tris, vec![[0, 2, 1], [0, 3, 2]]);
        assert_eq!(m.verts.len(), 4);
    }

    fn concave_prism() -> (Vec<[f64; 3]>, Vec<Vec<u32>>) {
        // A U outline (area 10, perimeter 22), extruded two units. The first
        // vertex cannot see the whole face, so a fan overlaps the notch.
        let outline = [
            [0.0, 0.0],
            [4.0, 0.0],
            [4.0, 4.0],
            [3.0, 4.0],
            [3.0, 1.0],
            [1.0, 1.0],
            [1.0, 4.0],
            [0.0, 4.0],
        ];
        let mut points = Vec::new();
        for z in [0.0, 2.0] {
            points.extend(outline.iter().map(|p| [p[0], p[1], z]));
        }
        let mut faces = vec![
            vec![0, 1, 2, 3, 4, 5, 6, 7],
            vec![15, 14, 13, 12, 11, 10, 9, 8],
        ];
        for i in 0..8_u32 {
            let j = (i + 1) % 8;
            faces.push(vec![i, i + 8, j + 8, j]);
        }
        (points, faces)
    }

    #[test]
    fn polyhedron_triangulates_concave_faces_without_overlap() {
        let (points, faces) = concave_prism();
        let mesh = polyhedron(&points, &faces);

        // OpenSCAD oracle for the same U prism: 28 triangles, volume 20,
        // surface area 64 (= two 10-unit caps + 22*2 side wall).
        assert_eq!(mesh.tris.len(), 28);
        assert!((mesh.volume() - 20.0).abs() < 1e-9);
        assert!((mesh.surface_area() - 64.0).abs() < 1e-9);
        assert!(mesh.signed_volume() > 0.0);
    }

    #[test]
    fn polyhedron_triangulates_concave_slanted_face() {
        let uv = [
            [0.0, 0.0],
            [4.0, 0.0],
            [4.0, 4.0],
            [3.0, 4.0],
            [3.0, 1.0],
            [1.0, 1.0],
            [1.0, 4.0],
            [0.0, 4.0],
        ];
        // Plane p(u,v) = (u,v,u+2v), whose area scale is |(1,0,1) ×
        // (0,1,2)| = sqrt(6). This exercises dominant-axis projection.
        let points: Vec<_> = uv.iter().map(|p| [p[0], p[1], p[0] + 2.0 * p[1]]).collect();
        let mesh = polyhedron(&points, &[vec![0, 1, 2, 3, 4, 5, 6, 7]]);
        assert_eq!(mesh.tris.len(), 6);
        assert!((mesh.surface_area() - 10.0 * 6.0_f64.sqrt()).abs() < 1e-9);
    }

    #[test]
    fn polyhedron_skips_invalid_degenerate_and_nonplanar_faces() {
        let points = vec![
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [2.0, 2.0, 0.0],
            [0.0, 2.0, 0.0],
            [1.0, 0.0, 0.0], // collinear with points 0 and 1
            [1.0, 3.0, 0.1], // visibly off the other points' plane
        ];
        let mesh = polyhedron(
            &points,
            &[
                vec![0, 1, 99],      // out-of-range index
                vec![0, 4, 1],       // zero-area face
                vec![0, 1, 2, 5, 3], // materially non-planar n-gon
                vec![0, 1, 2, 1, 3], // repeated/non-simple index
                vec![0, 1, 2],       // one valid face remains
            ],
        );
        assert_eq!(mesh.tris, vec![[0, 2, 1]]);
    }

    #[test]
    fn cone_is_manifold_shape() {
        let m = cylinder(10.0, 5.0, 0.0, false, spec(32.0));
        assert!(m.signed_volume() > 0.0);
        assert!(!m.verts.is_empty());
    }
}
