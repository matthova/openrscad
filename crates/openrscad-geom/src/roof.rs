//! `roof()` — lift a 2D profile to a straight-skeleton roof, where every point
//! rises at unit slope until it reaches the *straight skeleton* (the ridge lines
//! traced by the profile's edges as they march inward at equal speed). The apex
//! height at any point is its distance to the nearest edge.
//!
//! This implements the **convex** case: a single convex outer contour, whose
//! skeleton needs only *edge events* (an edge shrinks to nothing) and never a
//! *split event* (a reflex vertex slicing the wavefront in two). A concave
//! profile or one with holes needs the general algorithm and is out of scope, so
//! those are warned and skipped rather than silently mis-roofed.

use crate::mesh::Mesh;
use crate::shape2d::Contour;

pub const ROOF_UNSUPPORTED_WARNING: &str =
    "roof() supports a single convex profile without holes; skipping the rest";

/// Straight-skeleton roof of `contours`. Returns the mesh and an optional
/// warning when part of the input was skipped (concave, holed, or degenerate).
pub fn roof(contours: &[Contour]) -> (Mesh, Option<String>) {
    // Keep only non-degenerate rings; a convex roof takes exactly one.
    let rings: Vec<&Contour> = contours.iter().filter(|c| c.len() >= 3).collect();
    let unsupported = rings.len() != 1;
    let mesh = rings
        .first()
        .filter(|_| !unsupported)
        .and_then(|c| convex_roof(c))
        .unwrap_or_default();
    let warn = (unsupported || (rings.len() == 1 && mesh.verts.is_empty()))
        .then(|| ROOF_UNSUPPORTED_WARNING.to_string());
    (mesh, warn)
}

type P2 = [f64; 2];

fn sub(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}
fn cross(a: P2, b: P2) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn dot(a: P2, b: P2) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

/// Signed area (positive = CCW).
fn signed_area(poly: &[P2]) -> f64 {
    let n = poly.len();
    0.5 * (0..n)
        .map(|i| cross(poly[i], poly[(i + 1) % n]))
        .sum::<f64>()
}

/// Whether every turn has the same sign — a simple convexity test.
fn is_convex(poly: &[P2]) -> bool {
    let n = poly.len();
    let mut sign = 0.0;
    for i in 0..n {
        let a = sub(poly[(i + 1) % n], poly[i]);
        let b = sub(poly[(i + 2) % n], poly[(i + 1) % n]);
        let c = cross(a, b);
        if c.abs() > 1e-9 {
            if sign == 0.0 {
                sign = c;
            } else if c * sign < 0.0 {
                return false;
            }
        }
    }
    true
}

/// An active wavefront vertex: the moving intersection of two edge lines.
#[derive(Clone)]
struct Vert {
    pos: P2,
    t0: f64,
    vel: P2,
    prev: usize, // vertex index
    next: usize,
    left_edge: usize,  // original edge arriving at this vertex
    right_edge: usize, // original edge leaving this vertex
    alive: bool,
}

impl Vert {
    fn at(&self, t: f64) -> P2 {
        [
            self.pos[0] + self.vel[0] * (t - self.t0),
            self.pos[1] + self.vel[1] * (t - self.t0),
        ]
    }
}

/// Velocity of a vertex bounded by edges with inward unit normals `na`, `nb`:
/// the point moving so both offset lines stay satisfied (`w·n = 1` for each).
fn vertex_velocity(na: P2, nb: P2) -> P2 {
    let det = na[0] * nb[1] - na[1] * nb[0];
    if det.abs() < 1e-12 {
        // Straight (or nearly straight) angle: the two normals coincide, so the
        // vertex simply rides its own normal inward.
        return na;
    }
    // Solve [na; nb] w = [1; 1].
    [(nb[1] - na[1]) / det, (na[0] - nb[0]) / det]
}

fn convex_roof(contour: &Contour) -> Option<Mesh> {
    let mut poly: Vec<P2> = contour.to_vec();
    // Drop repeated/near-duplicate points that would break the normals.
    poly.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9);
    if poly.len() >= 2 && poly[0] == poly[poly.len() - 1] {
        poly.pop();
    }
    let n = poly.len();
    if n < 3 || !is_convex(&poly) {
        return None;
    }
    // Work with a CCW ring so the inward normal is the left normal.
    if signed_area(&poly) < 0.0 {
        poly.reverse();
    }

    // Inward unit normal of each edge i (poly[i] -> poly[i+1]).
    let normal: Vec<P2> = (0..n)
        .map(|i| {
            let d = sub(poly[(i + 1) % n], poly[i]);
            let len = d[0].hypot(d[1]);
            [-d[1] / len, d[0] / len]
        })
        .collect();

    // One roof face per original edge: base edge plus the skeleton nodes its two
    // ends trace. `right`/`left` accumulate those nodes at each endpoint.
    let base_z0 = |p: P2| [p[0], p[1], 0.0];
    let mut face_right: Vec<Vec<[f64; 3]>> = vec![Vec::new(); n];
    let mut face_left: Vec<Vec<[f64; 3]>> = vec![Vec::new(); n];
    let mut faces: Vec<Vec<[f64; 3]>> = Vec::new();

    // Initial wavefront: one vertex per original vertex i, between edge i-1 and i.
    let mut verts: Vec<Vert> = (0..n)
        .map(|i| {
            let le = (i + n - 1) % n;
            Vert {
                pos: poly[i],
                t0: 0.0,
                vel: vertex_velocity(normal[le], normal[i]),
                prev: (i + n - 1) % n,
                next: (i + 1) % n,
                left_edge: le,
                right_edge: i,
                alive: true,
            }
        })
        .collect();

    let node3d = |v: &Vert, t: f64| {
        let p = v.at(t);
        [p[0], p[1], t]
    };

    let mut alive = n;
    let mut guard = 0;
    while alive > 2 {
        guard += 1;
        if guard > 4 * n + 8 {
            return None; // numerical stall; bail rather than loop forever
        }
        // Find the next edge to collapse: the live edge (a, a.next) whose two
        // vertices meet soonest, strictly after their creation.
        let mut best: Option<(f64, usize)> = None;
        for a in 0..verts.len() {
            if !verts[a].alive {
                continue;
            }
            let b = verts[a].next;
            let (va, vb) = (&verts[a], &verts[b]);
            // Relative position C + D t of vb - va.
            let c = [
                (vb.pos[0] - vb.vel[0] * vb.t0) - (va.pos[0] - va.vel[0] * va.t0),
                (vb.pos[1] - vb.vel[1] * vb.t0) - (va.pos[1] - va.vel[1] * va.t0),
            ];
            let d = [vb.vel[0] - va.vel[0], vb.vel[1] - va.vel[1]];
            let dd = dot(d, d);
            if dd < 1e-18 {
                continue; // parallel motion, never collapses
            }
            let t = -dot(c, d) / dd;
            // Must be reached after both endpoints exist, and actually coincide.
            let resid = [c[0] + d[0] * t, c[1] + d[1] * t];
            let t_min = va.t0.max(vb.t0);
            let reached = t >= t_min - 1e-9 && resid[0].hypot(resid[1]) < 1e-6;
            if reached && best.map(|(bt, _)| t < bt).unwrap_or(true) {
                best = Some((t, a));
            }
        }
        let (t, a) = best?;
        let b = verts[a].next;
        let p = node3d(&verts[a], t);
        let e = verts[a].right_edge; // the collapsing edge, between a and b

        // Close the collapsing edge's face: base, up the right chain to the apex,
        // then back down the left chain.
        let mut face = vec![base_z0(poly[e]), base_z0(poly[(e + 1) % n])];
        face.extend(face_right[e].iter().copied());
        face.push(p);
        face.extend(face_left[e].iter().rev().copied());
        faces.push(face);

        // Merge a and b into a new vertex at p, bounded by the outer edges.
        let (la, rb) = (verts[a].left_edge, verts[b].right_edge);
        let (pa, nb) = (verts[a].prev, verts[b].next);
        // The new node joins the two surviving neighbour faces.
        face_right[la].push(p);
        face_left[rb].push(p);

        verts[a].alive = false;
        verts[b].alive = false;
        let k = verts.len();
        verts.push(Vert {
            pos: [p[0], p[1]],
            t0: t,
            vel: vertex_velocity(normal[la], normal[rb]),
            prev: pa,
            next: nb,
            left_edge: la,
            right_edge: rb,
            alive: true,
        });
        verts[pa].next = k;
        verts[nb].prev = k;
        alive -= 1;
    }

    // The last two edges meet along the final ridge; their chains already hold
    // both ridge endpoints, so close them without a new apex.
    for v in verts.iter().filter(|v| v.alive) {
        let e = v.right_edge;
        let mut face = vec![base_z0(poly[e]), base_z0(poly[(e + 1) % n])];
        face.extend(face_right[e].iter().copied());
        face.extend(face_left[e].iter().rev().copied());
        if face.len() >= 3 {
            faces.push(face);
        }
    }

    Some(build_mesh(&poly, &faces))
}

/// Assemble the base cap (facing down) and the sloped faces into one mesh.
fn build_mesh(base: &[P2], faces: &[Vec<[f64; 3]>]) -> Mesh {
    let mut mesh = Mesh::new();
    // Base cap: the profile at z=0, wound so its normal points down (-Z).
    let base3: Vec<[f64; 3]> = base.iter().map(|p| [p[0], p[1], 0.0]).collect();
    let base_idx = fan_indices(base.len());
    let start = mesh.verts.len() as u32;
    mesh.verts.extend_from_slice(&base3);
    for tri in base_idx.chunks(3) {
        // Reverse for a downward normal.
        mesh.tris
            .push([start + tri[2], start + tri[1], start + tri[0]]);
    }
    // Sloped faces: each planar polygon fanned from its first vertex.
    for face in faces {
        let mut poly = face.clone();
        poly.dedup_by(|a, b| dist3(*a, *b) < 1e-7);
        if poly.len() >= 2 && dist3(poly[0], poly[poly.len() - 1]) < 1e-7 {
            poly.pop();
        }
        if poly.len() < 3 {
            continue;
        }
        let s = mesh.verts.len() as u32;
        mesh.verts.extend_from_slice(&poly);
        for tri in fan_indices(poly.len()).chunks(3) {
            mesh.tris.push([s + tri[0], s + tri[1], s + tri[2]]);
        }
    }
    mesh
}

fn dist3(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Triangle-fan indices for an `n`-gon: (0,1,2),(0,2,3),…
fn fan_indices(n: usize) -> Vec<u32> {
    let mut out = Vec::new();
    for i in 1..n.saturating_sub(1) {
        out.push(0);
        out.push(i as u32);
        out.push((i + 1) as u32);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zmax(m: &Mesh) -> f64 {
        m.verts.iter().map(|v| v[2]).fold(0.0, f64::max)
    }

    #[test]
    fn square_roofs_to_a_central_apex() {
        // A 10×10 square rises to (5,5,5): a pyramid, height = half the side.
        let sq = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let (m, warn) = roof(&[sq]);
        assert!(warn.is_none());
        assert!((zmax(&m) - 5.0).abs() < 1e-9);
        assert!(m.verts.iter().any(|v| (v[0] - 5.0).abs() < 1e-9
            && (v[1] - 5.0).abs() < 1e-9
            && (v[2] - 5.0).abs() < 1e-9));
    }

    #[test]
    fn rectangle_roofs_to_a_ridge() {
        // A 10×6 rectangle ridges at height 3 (half the shorter side), spanning
        // x∈[3,7] at y=3.
        let r = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 6.0], [0.0, 6.0]];
        let (m, warn) = roof(&[r]);
        assert!(warn.is_none());
        assert!((zmax(&m) - 3.0).abs() < 1e-9);
        let ridge = |x: f64| {
            m.verts.iter().any(|v| {
                (v[0] - x).abs() < 1e-6 && (v[1] - 3.0).abs() < 1e-6 && (v[2] - 3.0).abs() < 1e-6
            })
        };
        assert!(ridge(3.0) && ridge(7.0));
    }

    #[test]
    fn concave_profile_is_skipped_with_a_warning() {
        // An L-shape has a reflex vertex (needs a split event), so it is warned.
        let l = vec![
            [0.0, 0.0],
            [20.0, 0.0],
            [20.0, 8.0],
            [8.0, 8.0],
            [8.0, 20.0],
            [0.0, 20.0],
        ];
        let (m, warn) = roof(&[l]);
        assert!(warn.is_some());
        assert!(m.verts.is_empty());
    }
}
