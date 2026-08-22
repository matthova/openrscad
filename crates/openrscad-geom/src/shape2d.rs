//! 2D shapes (contours), polygon triangulation, 2D boolean ops, and 2D→3D
//! extrusion.
//!
//! A 2D node renders to a set of closed contours (`Vec<Contour>`) using even-odd
//! nesting (outers + holes). Boolean ops (union/difference/intersection) go
//! through the `geo` polygon clipper via [`boolean_2d`]; projection silhouettes
//! via [`silhouette`].

use crate::mesh::Mesh;
use crate::tessellate::fragments;
use geo::{BooleanOps, LineString, MultiPolygon, Polygon};
use openrscad_ir::{FragmentSpec, Node};
use std::f64::consts::PI;

pub type Point2 = [f64; 2];
pub type Contour = Vec<Point2>;

/// A 2D boolean operation. (Union goes through [`union_all`], which handles the
/// n-ary case directly, so it is not represented here.)
#[derive(Clone, Copy)]
pub enum Bop {
    Difference,
    Intersection,
}

// ---- contour <-> geo conversions -----------------------------------------

fn to_linestring(c: &Contour) -> LineString<f64> {
    LineString::from(c.iter().map(|p| (p[0], p[1])).collect::<Vec<_>>())
}

fn from_linestring(ls: &LineString<f64>) -> Contour {
    let mut c: Contour = ls.0.iter().map(|p| [p.x, p.y]).collect();
    if c.len() > 1 && c.first() == c.last() {
        c.pop(); // geo rings are closed; drop the duplicate
    }
    c
}

/// Group flat even-odd contours into `(outer, holes)` polygons (outers oriented
/// CCW, holes CW) — the model the clipper needs.
fn group_contours(contours: &[Contour]) -> Vec<(Contour, Vec<Contour>)> {
    let valid: Vec<&Contour> = contours.iter().filter(|c| c.len() >= 3).collect();
    let n = valid.len();
    if n == 0 {
        return Vec::new();
    }
    let rep: Vec<Point2> = valid.iter().map(|c| c[0]).collect();
    let depth: Vec<usize> = (0..n)
        .map(|i| {
            (0..n)
                .filter(|&j| j != i && point_in_polygon(valid[j], rep[i]))
                .count()
        })
        .collect();
    let orient = |c: &Contour, ccw: bool| -> Contour {
        if (signed_area(c) > 0.0) == ccw {
            c.clone()
        } else {
            c.iter().rev().cloned().collect()
        }
    };
    let mut groups: Vec<(Contour, Vec<Contour>)> = Vec::new();
    let mut group_of = vec![None; n];
    for i in 0..n {
        if depth[i] % 2 == 0 {
            group_of[i] = Some(groups.len());
            groups.push((orient(valid[i], true), Vec::new()));
        }
    }
    for h in 0..n {
        if depth[h] % 2 == 1 {
            if let Some(p) =
                (0..n).find(|&p| depth[p] + 1 == depth[h] && point_in_polygon(valid[p], rep[h]))
            {
                if let Some(g) = group_of[p] {
                    groups[g].1.push(orient(valid[h], false));
                }
            }
        }
    }
    groups
}

fn to_multipolygon(contours: &[Contour]) -> MultiPolygon<f64> {
    let polys = group_contours(contours)
        .into_iter()
        .map(|(outer, holes)| {
            Polygon::new(
                to_linestring(&outer),
                holes.iter().map(to_linestring).collect(),
            )
        })
        .collect();
    MultiPolygon::new(polys)
}

fn from_multipolygon(mp: MultiPolygon<f64>) -> Vec<Contour> {
    let mut out = Vec::new();
    for poly in mp {
        out.push(from_linestring(poly.exterior()));
        for hole in poly.interiors() {
            out.push(from_linestring(hole));
        }
    }
    out.retain(|c| c.len() >= 3);
    out
}

// ---- 2D boolean ops -------------------------------------------------------

/// Apply a boolean op between two contour sets, returning result contours
/// (outers + holes, even-odd).
pub fn boolean_2d(a: &[Contour], b: &[Contour], op: Bop) -> Vec<Contour> {
    let (ma, mb) = (to_multipolygon(a), to_multipolygon(b));
    let r = match op {
        Bop::Difference => ma.difference(&mb),
        Bop::Intersection => ma.intersection(&mb),
    };
    from_multipolygon(r)
}

/// Balanced (divide-and-conquer) union of many multipolygons — far faster than a
/// linear fold when unioning e.g. every triangle of a projection.
fn union_multi(mut items: Vec<MultiPolygon<f64>>) -> Option<MultiPolygon<f64>> {
    if items.is_empty() {
        return None;
    }
    while items.len() > 1 {
        let mut next = Vec::with_capacity(items.len().div_ceil(2));
        let mut it = items.into_iter();
        while let Some(a) = it.next() {
            match it.next() {
                Some(b) => next.push(a.union(&b)),
                None => next.push(a),
            }
        }
        items = next;
    }
    items.into_iter().next()
}

/// Union of several contour sets (e.g. the children of a 2D `union`).
pub fn union_all(sets: &[Vec<Contour>]) -> Vec<Contour> {
    let mps: Vec<_> = sets
        .iter()
        .filter(|s| !s.is_empty())
        .map(|s| to_multipolygon(s))
        .collect();
    union_multi(mps).map(from_multipolygon).unwrap_or_default()
}

/// The z=0 silhouette of a mesh (`projection(cut=false)`): union every triangle
/// projected onto the XY plane.
pub fn silhouette(mesh: &Mesh) -> Vec<Contour> {
    let mut polys = Vec::new();
    for t in &mesh.tris {
        let a = mesh.verts[t[0] as usize];
        let b = mesh.verts[t[1] as usize];
        let c = mesh.verts[t[2] as usize];
        let area = ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs();
        if area < 1e-9 {
            continue; // edge-on triangle projects to a line
        }
        polys.push(MultiPolygon::new(vec![Polygon::new(
            LineString::from(vec![(a[0], a[1]), (b[0], b[1]), (c[0], c[1])]),
            vec![],
        )]));
    }
    union_multi(polys)
        .map(from_multipolygon)
        .unwrap_or_default()
}

/// 2D convex hull (Andrew's monotone chain) of a point set → one CCW contour.
pub fn hull_2d(mut pts: Vec<Point2>) -> Vec<Contour> {
    pts.sort_by(|a, b| {
        a[0].partial_cmp(&b[0])
            .unwrap()
            .then(a[1].partial_cmp(&b[1]).unwrap())
    });
    pts.dedup();
    if pts.len() < 3 {
        return Vec::new();
    }
    let cross = |o: Point2, a: Point2, b: Point2| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut lower: Vec<Point2> = Vec::new();
    for &p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<Point2> = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    vec![lower]
}

/// Render a 2D subtree to contours.
pub fn render2d(node: &Node) -> Vec<Contour> {
    match node {
        Node::Empty => Vec::new(),
        Node::Square { size, center } => vec![square_contour(*size, *center)],
        Node::Circle { r, frags } => vec![circle_contour(*r, *frags)],
        Node::Polygon { points, paths } => polygon_contours(points, paths),
        Node::Import {
            data,
            format,
            layer,
            id,
            origin,
            scale,
            dpi,
        } => {
            let contours = match format.as_str() {
                "dxf" => crate::vector2d::import_dxf(data, layer.as_deref()),
                "svg" => crate::vector2d::import_svg(data, layer.as_deref(), id.as_deref(), *dpi),
                _ => Vec::new(),
            };
            // `origin` and `scale` place the imported outline; both are 2D-only
            // upstream, so the 3D formats above never see them.
            if *origin == [0.0, 0.0] && *scale == 1.0 {
                contours
            } else {
                contours
                    .into_iter()
                    .map(|c| {
                        c.into_iter()
                            .map(|p| [(p[0] - origin[0]) * scale, (p[1] - origin[1]) * scale])
                            .collect()
                    })
                    .collect()
            }
        }
        Node::Offset {
            r,
            delta,
            chamfer,
            frags,
            child,
        } => offset(&render2d(child), *r, *delta, *chamfer, *frags),
        Node::Translate { v, child } => {
            map_contours(render2d(child), |p| [p[0] + v[0], p[1] + v[1]])
        }
        Node::Scale { v, child } => map_contours(render2d(child), |p| [p[0] * v[0], p[1] * v[1]]),
        Node::Rotate { deg, child } => {
            let a = deg[2].to_radians();
            let (s, c) = (a.sin(), a.cos());
            map_contours(render2d(child), |p| {
                [p[0] * c - p[1] * s, p[0] * s + p[1] * c]
            })
        }
        // 2D reflection across the line through the origin with normal (v.x, v.y).
        Node::Mirror { v, child } => {
            let d = v[0] * v[0] + v[1] * v[1];
            if d == 0.0 {
                render2d(child)
            } else {
                map_contours(render2d(child), |p| {
                    let t = 2.0 * (p[0] * v[0] + p[1] * v[1]) / d;
                    [p[0] - t * v[0], p[1] - t * v[1]]
                })
            }
        }
        // 2D affine: the top-left 2×2 plus the translation column.
        Node::MultMatrix { m, child } => map_contours(render2d(child), |p| {
            [
                m[0][0] * p[0] + m[0][1] * p[1] + m[0][3],
                m[1][0] * p[0] + m[1][1] * p[1] + m[1][3],
            ]
        }),
        Node::Resize { new, auto, child } => resize2d(render2d(child), *new, *auto),
        // Union/group: clip overlaps (proper 2D union).
        Node::Group(children) | Node::Union(children) => {
            let sets: Vec<Vec<Contour>> = children.iter().map(render2d).collect();
            union_all(&sets)
        }
        Node::Difference(children) => {
            let Some((first, rest)) = children.split_first() else {
                return Vec::new();
            };
            let a = render2d(first);
            if rest.is_empty() {
                return a;
            }
            let subtract = union_all(&rest.iter().map(render2d).collect::<Vec<_>>());
            boolean_2d(&a, &subtract, Bop::Difference)
        }
        Node::Intersection(children) => {
            let mut it = children.iter().map(render2d);
            let Some(mut acc) = it.next() else {
                return Vec::new();
            };
            for c in it {
                acc = boolean_2d(&acc, &c, Bop::Intersection);
            }
            acc
        }
        Node::Hull(children) => {
            let pts: Vec<Point2> = children.iter().flat_map(render2d).flatten().collect();
            hull_2d(pts)
        }
        Node::Minkowski(children) => {
            let sets: Vec<Vec<Contour>> = children
                .iter()
                .map(render2d)
                .filter(|s| !s.is_empty())
                .collect();
            minkowski_2d(sets)
        }
        // Display attributes and provenance are transparent to 2D geometry; `%`
        // background is excluded from the fused/exported profile.
        Node::Color { child, .. } | Node::Highlight(child) | Node::Provenance { child, .. } => {
            render2d(child)
        }
        Node::Background(_) => Vec::new(),
        // A projection anywhere in a 2D subtree flattens its 3D child; rendered
        // via the geometry layer, not here (needs a mesh). Handled by render_node.
        _ => Vec::new(),
    }
}

/// Exact 2D Minkowski sum of several contour sets. Minkowski distributes over
/// union, and a triangle⊕triangle is convex (the hull of the 9 vertex sums is
/// exact), so decompose each operand into triangles (earcut), sum every pair,
/// and union the pieces — correct for non-convex operands (e.g. rounding an
/// L-outline or gear), not just the convex hull.
fn minkowski_2d(sets: Vec<Vec<Contour>>) -> Vec<Contour> {
    let mut it = sets.into_iter().filter(|s| !s.is_empty());
    let Some(mut acc) = it.next() else {
        return Vec::new();
    };
    for s in it {
        acc = minkowski_pair_2d(&acc, &s);
        if acc.is_empty() {
            break;
        }
    }
    acc
}

/// The convex triangles of a contour set (earcut, holes cut out).
fn triangles_2d(contours: &[Contour]) -> Vec<[Point2; 3]> {
    let (points, _ranges, tris) = prepare(contours);
    tris.iter()
        .map(|t| {
            [
                points[t[0] as usize],
                points[t[1] as usize],
                points[t[2] as usize],
            ]
        })
        .collect()
}

fn minkowski_pair_2d(a: &[Contour], b: &[Contour]) -> Vec<Contour> {
    let (ta, tb) = (triangles_2d(a), triangles_2d(b));
    if ta.is_empty() || tb.is_empty() {
        return Vec::new();
    }
    let mut pieces: Vec<MultiPolygon<f64>> = Vec::new();
    for x in &ta {
        for y in &tb {
            let mut sums = Vec::with_capacity(9);
            for p in x {
                for q in y {
                    sums.push([p[0] + q[0], p[1] + q[1]]);
                }
            }
            for c in hull_2d(sums) {
                if c.len() >= 3 {
                    pieces.push(MultiPolygon::new(vec![Polygon::new(
                        to_linestring(&c),
                        vec![],
                    )]));
                }
            }
        }
    }
    union_multi(pieces)
        .map(from_multipolygon)
        .unwrap_or_default()
}

fn map_contours(cs: Vec<Contour>, f: impl Fn(Point2) -> Point2) -> Vec<Contour> {
    cs.into_iter()
        .map(|c| c.into_iter().map(&f).collect())
        .collect()
}

/// 2D `resize`: scale the contours so their bounding box matches `new` (0 = keep;
/// an `auto` axis with no target adopts the first explicit factor).
fn resize2d(contours: Vec<Contour>, new: [f64; 3], auto: [bool; 3]) -> Vec<Contour> {
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for c in &contours {
        for p in c {
            for i in 0..2 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
    }
    if lo[0] > hi[0] {
        return contours;
    }
    let size = [hi[0] - lo[0], hi[1] - lo[1]];
    let mut factor = [1.0; 2];
    let mut explicit = None;
    for i in 0..2 {
        if new[i] > 0.0 && size[i] > 0.0 {
            factor[i] = new[i] / size[i];
            explicit.get_or_insert(factor[i]);
        }
    }
    if let Some(f) = explicit {
        for i in 0..2 {
            if new[i] == 0.0 && auto[i] {
                factor[i] = f;
            }
        }
    }
    map_contours(contours, |p| [p[0] * factor[0], p[1] * factor[1]])
}

fn square_contour(size: Point2, center: bool) -> Contour {
    let (x0, y0) = if center {
        (-size[0] / 2.0, -size[1] / 2.0)
    } else {
        (0.0, 0.0)
    };
    let (x1, y1) = (x0 + size[0], y0 + size[1]);
    vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]] // CCW
}

fn circle_contour(r: f64, frags: FragmentSpec) -> Contour {
    let n = fragments(r, frags).max(3);
    (0..n)
        .map(|i| {
            let a = 2.0 * PI * i as f64 / n as f64;
            [r * a.cos(), r * a.sin()]
        })
        .collect()
}

fn polygon_contours(points: &[Point2], paths: &Option<Vec<Vec<u32>>>) -> Vec<Contour> {
    match paths {
        Some(paths) => paths
            .iter()
            .map(|path| path.iter().map(|&i| points[i as usize]).collect())
            .collect(),
        None => vec![points.to_vec()],
    }
}

/// Cross-section of a mesh at the z=0 plane (`projection(cut=true)`): returns
/// the closed contours where the mesh crosses the plane.
pub fn slice_z0(mesh: &Mesh) -> Vec<Contour> {
    // Slice slightly above 0 to avoid coplanar-face degeneracies.
    const Z: f64 = 1e-7;
    let mut segs: Vec<(Point2, Point2)> = Vec::new();
    for t in &mesh.tris {
        let v = [
            mesh.verts[t[0] as usize],
            mesh.verts[t[1] as usize],
            mesh.verts[t[2] as usize],
        ];
        let mut cross = Vec::new();
        for &(a, b) in &[(0, 1), (1, 2), (2, 0)] {
            let (za, zb) = (v[a][2] - Z, v[b][2] - Z);
            if (za < 0.0) != (zb < 0.0) {
                let f = za / (za - zb);
                cross.push([
                    v[a][0] + (v[b][0] - v[a][0]) * f,
                    v[a][1] + (v[b][1] - v[a][1]) * f,
                ]);
            }
        }
        if cross.len() == 2 {
            segs.push((cross[0], cross[1]));
        }
    }
    chain_segments(segs)
}

/// Chain unordered segments into closed contours by walking segment by segment
/// (so points shared by collinear segments are handled correctly).
pub(crate) fn chain_segments(segs: Vec<(Point2, Point2)>) -> Vec<Contour> {
    let key = |p: Point2| [(p[0] * 1e5).round() as i64, (p[1] * 1e5).round() as i64];
    // point key -> indices of incident segments
    let mut inc: std::collections::HashMap<[i64; 2], Vec<usize>> = Default::default();
    for (i, (a, b)) in segs.iter().enumerate() {
        inc.entry(key(*a)).or_default().push(i);
        inc.entry(key(*b)).or_default().push(i);
    }
    let mut used = vec![false; segs.len()];
    let mut contours = Vec::new();
    for start in 0..segs.len() {
        if used[start] {
            continue;
        }
        let mut contour = Vec::new();
        let mut si = start;
        let mut cur = segs[si].0;
        let start_key = key(cur);
        loop {
            used[si] = true;
            contour.push(cur);
            // step to the other endpoint of the current segment
            cur = if key(segs[si].0) == key(cur) {
                segs[si].1
            } else {
                segs[si].0
            };
            if key(cur) == start_key {
                break; // closed loop
            }
            // next unused segment incident to `cur`
            match inc
                .get(&key(cur))
                .and_then(|v| v.iter().find(|&&j| !used[j]).copied())
            {
                Some(j) => si = j,
                None => break,
            }
        }
        if contour.len() >= 3 {
            contours.push(contour);
        }
    }
    contours
}

/// 2D offset of contours. `r` rounds convex corners; `delta` mitres (or
/// chamfers). Positive grows, negative shrinks.
///
/// The offset region is assembled from **convex pieces** (an offset slab per
/// edge plus a join cap per corner) unioned/subtracted through the `geo`
/// clipper, rather than a single per-vertex ring — a folding ring
/// self-intersects on concave insets and would fill the bowtie. Growing is
/// `solid ∪ band`; shrinking is `solid − band`, so an inset larger than a local
/// feature collapses to empty (matching OpenSCAD) instead of a wrong solid.
pub fn offset(
    contours: &[Contour],
    r: f64,
    delta: f64,
    chamfer: bool,
    frags: FragmentSpec,
) -> Vec<Contour> {
    let (amt, rounded) = if r != 0.0 { (r, true) } else { (delta, false) };
    let solid = to_multipolygon(contours);
    if amt == 0.0 {
        return from_multipolygon(solid);
    }
    // Orient each boundary (outer CCW, holes CW) so the edge normal points out
    // of the solid; the band then grows/shrinks holes correctly too.
    let mut pieces: Vec<Contour> = Vec::new();
    for (outer, holes) in group_contours(contours) {
        offset_pieces(&outer, amt, rounded, chamfer, frags, &mut pieces);
        for h in &holes {
            offset_pieces(h, amt, rounded, chamfer, frags, &mut pieces);
        }
    }
    let band = clean_union(&pieces);
    let result = if amt > 0.0 {
        solid.union(&band)
    } else {
        solid.difference(&band)
    };
    from_multipolygon(result)
}

/// Union a set of convex pieces (each taken as a filled CCW region) through the
/// clipper into a clean multipolygon. The pieces are convex, so no
/// self-intersection can arise.
fn clean_union(pieces: &[Contour]) -> MultiPolygon<f64> {
    let mps: Vec<MultiPolygon<f64>> = pieces
        .iter()
        .filter(|c| c.len() >= 3 && signed_area(c).abs() > 1e-12)
        .map(|c| {
            let ccw: Contour = if signed_area(c) < 0.0 {
                c.iter().rev().cloned().collect()
            } else {
                c.clone()
            };
            MultiPolygon::new(vec![Polygon::new(to_linestring(&ccw), vec![])])
        })
        .collect();
    union_multi(mps).unwrap_or_else(|| MultiPolygon::new(Vec::new()))
}

/// Emit the convex offset pieces for one oriented boundary contour into `out`:
/// an offset slab quad per edge, plus a join cap (round arc / miter / chamfer)
/// at each corner whose outer side gaps open. `poly` must be oriented so that
/// its right-hand edge normal points out of the solid (outer CCW, holes CW);
/// `amt` is signed (outward slabs when growing, inward when shrinking).
fn offset_pieces(
    poly: &[Point2],
    amt: f64,
    rounded: bool,
    chamfer: bool,
    frags: FragmentSpec,
    out: &mut Vec<Contour>,
) {
    let n = poly.len();
    if n < 3 {
        return;
    }
    let seg_full = fragments(amt.abs(), frags).max(3) as f64;

    let edge_normal = |i: usize| -> Point2 {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        let d = [b[0] - a[0], b[1] - a[1]];
        let len = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-12);
        [d[1] / len, -d[0] / len]
    };

    // One slab per edge: the edge and its offset copy.
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        let nrm = edge_normal(i);
        let ao = [a[0] + amt * nrm[0], a[1] + amt * nrm[1]];
        let bo = [b[0] + amt * nrm[0], b[1] + amt * nrm[1]];
        out.push(vec![a, b, bo, ao]);
    }

    // One join cap per corner that gaps open on its outer side (convex when
    // growing, reflex when shrinking) — reflex/convex on the other side is
    // already covered by the overlapping slabs.
    for i in 0..n {
        let vi = poly[i];
        let n_in = edge_normal((i + n - 1) % n);
        let n_out = edge_normal(i);
        let p_in = [vi[0] + amt * n_in[0], vi[1] + amt * n_in[1]];
        let p_out = [vi[0] + amt * n_out[0], vi[1] + amt * n_out[1]];

        let din = [
            vi[0] - poly[(i + n - 1) % n][0],
            vi[1] - poly[(i + n - 1) % n][1],
        ];
        let dout = [poly[(i + 1) % n][0] - vi[0], poly[(i + 1) % n][1] - vi[1]];
        let convex = din[0] * dout[1] - din[1] * dout[0] > 0.0;
        let fill = (amt > 0.0 && convex) || (amt < 0.0 && !convex);
        if !fill {
            continue;
        }

        if rounded {
            let a0 = n_in[1].atan2(n_in[0]);
            let a1 = n_out[1].atan2(n_out[0]);
            let mut da = a1 - a0;
            while da <= -PI {
                da += 2.0 * PI;
            }
            while da > PI {
                da -= 2.0 * PI;
            }
            let steps = ((seg_full * (da.abs() / (2.0 * PI))).ceil() as usize).max(1);
            let mut cap = vec![vi];
            for s in 0..=steps {
                let a = a0 + da * (s as f64 / steps as f64);
                cap.push([vi[0] + amt * a.cos(), vi[1] + amt * a.sin()]);
            }
            out.push(cap);
        } else if chamfer {
            out.push(vec![vi, p_in, p_out]);
        } else {
            // miter: apex is where the two offset edge-lines meet.
            match line_intersect(p_in, din, p_out, dout) {
                Some(m) => out.push(vec![vi, p_in, m, p_out]),
                None => out.push(vec![vi, p_in, p_out]),
            }
        }
    }
}

/// Intersection of line (p1, dir d1) with line (p2, dir d2).
fn line_intersect(p1: Point2, d1: Point2, p2: Point2, d2: Point2) -> Option<Point2> {
    let denom = d1[0] * d2[1] - d1[1] * d2[0];
    if denom.abs() < 1e-12 {
        return None;
    }
    let t = ((p2[0] - p1[0]) * d2[1] - (p2[1] - p1[1]) * d2[0]) / denom;
    Some([p1[0] + t * d1[0], p1[1] + t * d1[1]])
}

/// Signed area of a contour (positive when counter-clockwise).
fn signed_area(c: &[Point2]) -> f64 {
    let mut a = 0.0;
    for i in 0..c.len() {
        let p = c[i];
        let q = c[(i + 1) % c.len()];
        a += p[0] * q[1] - q[0] * p[1];
    }
    a / 2.0
}

/// Ear-clipping triangulation of a single simple polygon. Returns index
/// triples into `poly`. Assumes no holes; input is made CCW.
fn triangulate_simple(poly: &[Point2]) -> Vec<[usize; 3]> {
    let n = poly.len();
    if n < 3 {
        return Vec::new();
    }
    // Work on an index list, CCW.
    let mut idx: Vec<usize> = (0..n).collect();
    if signed_area(poly) < 0.0 {
        idx.reverse();
    }

    let cross = |o: Point2, a: Point2, b: Point2| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let in_tri = |p: Point2, a: Point2, b: Point2, c: Point2| {
        let d1 = cross(a, b, p);
        let d2 = cross(b, c, p);
        let d3 = cross(c, a, p);
        let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
        let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
        !(neg && pos)
    };

    let mut tris = Vec::new();
    let mut guard = 0;
    while idx.len() > 3 {
        let m = idx.len();
        let mut clipped = false;
        for i in 0..m {
            let ia = idx[(i + m - 1) % m];
            let ib = idx[i];
            let ic = idx[(i + 1) % m];
            let (a, b, c) = (poly[ia], poly[ib], poly[ic]);
            if cross(a, b, c) <= 0.0 {
                continue; // reflex or degenerate
            }
            // no other vertex inside this ear
            let mut ear = true;
            for &j in &idx {
                if j == ia || j == ib || j == ic {
                    continue;
                }
                if in_tri(poly[j], a, b, c) {
                    ear = false;
                    break;
                }
            }
            if ear {
                tris.push([ia, ib, ic]);
                idx.remove(i);
                clipped = true;
                break;
            }
        }
        guard += 1;
        if !clipped || guard > n + 5 {
            break; // degenerate; stop
        }
    }
    if idx.len() == 3 {
        tris.push([idx[0], idx[1], idx[2]]);
    }
    tris
}

/// Is `pt` inside the simple polygon `poly` (ray-cast, even-odd)?
fn point_in_polygon(poly: &[Point2], pt: Point2) -> bool {
    let n = poly.len();
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (pi, pj) = (poly[i], poly[j]);
        if ((pi[1] > pt[1]) != (pj[1] > pt[1]))
            && (pt[0] < (pj[0] - pi[0]) * (pt[1] - pi[1]) / (pj[1] - pi[1]) + pi[0])
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// The concatenated vertex list, each contour's `(start, len)` range in it, and
/// the cap triangulation (indices into the vertex list). See [`prepare`].
type PreparedContours = (Vec<Point2>, Vec<(usize, usize)>, Vec<[u32; 3]>);

/// Remove consecutive duplicate points (within the kernel's weld tolerance),
/// including a final point that repeats the first, so the contour has no
/// zero-length edges.
fn dedup_consecutive(c: &Contour) -> Contour {
    const EPS: f64 = 1e-7;
    let close = |a: &Point2, b: &Point2| (a[0] - b[0]).abs() <= EPS && (a[1] - b[1]).abs() <= EPS;
    let mut out: Contour = Vec::with_capacity(c.len());
    for p in c {
        if out.last().is_none_or(|q| !close(q, p)) {
            out.push(*p);
        }
    }
    if out.len() >= 2 && close(&out[0], out.last().unwrap()) {
        out.pop();
    }
    out
}

/// Prepare a set of contours (with even-odd nesting → outers + holes) for
/// filling and extrusion. Returns the concatenated vertex list, each contour's
/// `(start, len)` range in it (outers oriented CCW, holes CW), and the cap
/// triangulation (indices into the vertex list), with holes cut out via earcut.
fn prepare(contours: &[Contour]) -> PreparedContours {
    // Drop consecutive duplicate vertices (and any closing repeat of the first)
    // from each contour. A zero-length edge would otherwise extrude into a
    // degenerate side wall — a quad with two coincident corners — which the
    // manifold kernel rejects as non-manifold. Generated profiles routinely
    // emit such duplicates (e.g. BOSL2's `rack2d`).
    let cleaned: Vec<Contour> = contours.iter().map(dedup_consecutive).collect();
    let valid: Vec<&Contour> = cleaned.iter().filter(|c| c.len() >= 3).collect();
    let n = valid.len();
    if n == 0 {
        return (Vec::new(), Vec::new(), Vec::new());
    }
    // Nesting depth of each contour (how many others contain a point of it).
    let rep: Vec<Point2> = valid.iter().map(|c| c[0]).collect();
    let depth: Vec<usize> = (0..n)
        .map(|i| {
            (0..n)
                .filter(|&j| j != i && point_in_polygon(valid[j], rep[i]))
                .count()
        })
        .collect();

    // Orient: outers (even depth) CCW, holes (odd depth) CW.
    let oriented: Vec<Contour> = valid
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let want_ccw = depth[i] % 2 == 0;
            if (signed_area(c) > 0.0) == want_ccw {
                (*c).clone()
            } else {
                c.iter().rev().cloned().collect()
            }
        })
        .collect();

    let mut points: Vec<Point2> = Vec::new();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for c in &oriented {
        ranges.push((points.len(), c.len()));
        points.extend_from_slice(c);
    }

    // Cap triangulation: earcut each outer with its immediate holes.
    let mut cap_tris: Vec<[u32; 3]> = Vec::new();
    for i in 0..n {
        if depth[i] % 2 != 0 {
            continue; // hole; handled by its parent outer
        }
        let holes: Vec<usize> = (0..n)
            .filter(|&h| depth[h] == depth[i] + 1 && point_in_polygon(&oriented[i], rep[h]))
            .collect();

        let mut flat: Vec<f64> = Vec::new();
        let mut map: Vec<u32> = Vec::new(); // group vertex index -> global index
        let mut hole_starts: Vec<usize> = Vec::new();
        let push_ring = |flat: &mut Vec<f64>, map: &mut Vec<u32>, idx: usize| {
            let (s, len) = ranges[idx];
            for k in 0..len {
                let g = (s + k) as u32;
                flat.push(points[g as usize][0]);
                flat.push(points[g as usize][1]);
                map.push(g);
            }
        };
        push_ring(&mut flat, &mut map, i);
        for &h in &holes {
            hole_starts.push(map.len());
            push_ring(&mut flat, &mut map, h);
        }
        if let Ok(idx) = earcutr::earcut(&flat, &hole_starts, 2) {
            for t in idx.chunks(3) {
                if t.len() == 3 {
                    cap_tris.push([map[t[0]], map[t[1]], map[t[2]]]);
                }
            }
        }
    }
    (points, ranges, cap_tris)
}

/// A flat mesh of a 2D shape at z=0 (used when a 2D node is the render target),
/// with holes cut out (even-odd).
pub fn flat_mesh(contours: &[Contour]) -> Mesh {
    let (points, _ranges, cap_tris) = prepare(contours);
    let mut mesh = Mesh::new();
    mesh.verts = points.iter().map(|p| [p[0], p[1], 0.0]).collect();
    mesh.tris = cap_tris;
    mesh
}

/// Segments each edge of one closed contour is split into, matching OpenSCAD
/// 2024.12.
///
/// The outline gets a budget of `C` segments — `segments=` if given, else `$fn`,
/// else `360/$fa` — apportioned across edges in proportion to their length. An
/// edge whose share is below one segment still gets one and leaves the pool,
/// shrinking the budget for the rest. Whole segments are handed out first and
/// the remainder goes to the largest fractional shares; a tie wider than the
/// remaining budget is left unawarded, so an equilateral outline (every share
/// exactly `x.5`) rounds *down* rather than picking arbitrary edges.
///
/// `$fs` then caps each edge independently at `ceil(len / $fs)`, but only when
/// `$fa` drove the budget — `$fn` and `segments=` are exact requests.
///
/// Without twist there is no refinement at all unless `segments=` asks for it:
/// a straight prism's walls are planar, so extra points would only add
/// triangles. This is why `$fn=40` alone leaves `square(10)` a 4-gon.
fn contour_segments(lens: &[f64], segments: u32, frags: FragmentSpec, refining: bool) -> Vec<u32> {
    let n = lens.len();
    let budget_total = if segments > 0 {
        segments as f64
    } else if !refining {
        return vec![1; n];
    } else if frags.fn_ > 0.0 {
        frags.fn_
    } else if frags.fa > 0.0 {
        360.0 / frags.fa
    } else {
        return vec![1; n];
    };

    let mut out = vec![0u32; n];
    let mut pool: Vec<usize> = (0..n).filter(|&i| lens[i] > 0.0).collect();
    for &i in &(0..n).filter(|&i| lens[i] <= 0.0).collect::<Vec<_>>() {
        out[i] = 1;
    }
    let mut budget = budget_total;
    // Edges too short to earn a whole segment take one and leave the pool.
    loop {
        let perim: f64 = pool.iter().map(|&i| lens[i]).sum();
        if pool.is_empty() || perim <= 0.0 {
            break;
        }
        let under: Vec<usize> = pool
            .iter()
            .copied()
            .filter(|&i| budget * lens[i] / perim < 1.0)
            .collect();
        if under.is_empty() {
            break;
        }
        for i in under {
            out[i] = 1;
            pool.retain(|&j| j != i);
            budget -= 1.0;
        }
    }
    if !pool.is_empty() {
        let perim: f64 = pool.iter().map(|&i| lens[i]).sum();
        let quota: Vec<f64> = pool.iter().map(|&i| budget * lens[i] / perim).collect();
        for (k, &i) in pool.iter().enumerate() {
            out[i] = (quota[k].floor() as u32).max(1);
        }
        let mut remaining =
            budget.round() as i64 - pool.iter().map(|&i| out[i] as i64).sum::<i64>();
        let mut avail: Vec<usize> = (0..pool.len()).collect();
        while remaining > 0 && !avail.is_empty() {
            let top = avail
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, |m, k| m.max(quota[k].fract()));
            let winners: Vec<usize> = avail
                .iter()
                .copied()
                .filter(|&k| (quota[k].fract() - top).abs() < 1e-9)
                .collect();
            // A tie too wide for what is left goes unawarded, and the outline
            // ends up below its budget.
            if winners.len() as i64 > remaining {
                break;
            }
            for k in winners {
                out[pool[k]] += 1;
                avail.retain(|&j| j != k);
                remaining -= 1;
            }
        }
    }
    if segments == 0 && frags.fn_ <= 0.0 && frags.fs > 0.0 && refining {
        for i in 0..n {
            let cap = (lens[i] / frags.fs).ceil().max(1.0) as u32;
            out[i] = out[i].min(cap).max(1);
        }
    }
    out
}

/// Whether a `scale` bends the walls. A uniform scale keeps every wall planar —
/// a frustum's faces are flat — so OpenSCAD refines nothing for it, however far
/// from 1 it is. A non-uniform one does bend them, and is refined like a twist.
fn non_uniform(scale: Point2) -> bool {
    scale[0] != scale[1]
}

/// Resample every contour so each edge carries its [`contour_segments`] count.
///
/// The length an edge is measured by depends on what is bending the wall, and
/// only two of the three cases are known:
///
/// * twisting — the edge's own length, verified against the oracle;
/// * non-uniform scale alone — `max(original, scaled)`, likewise verified: the
///   edge that stretches earns proportionally more segments;
/// * both at once — upstream weights the edges the *other* way round (the edge
///   that shrinks gets more), which no measured rule yet explains, so this keeps
///   the twist-only lengths rather than guessing. See A-G10 in
///   `docs/compat-atoms.md`.
fn refine_contours(
    contours: &[Contour],
    segments: u32,
    frags: FragmentSpec,
    twist: f64,
    scale: Point2,
) -> Vec<Contour> {
    let twisting = twist != 0.0;
    let refining = twisting || non_uniform(scale);
    contours
        .iter()
        .map(|c| {
            if c.len() < 2 {
                return c.clone();
            }
            let n = c.len();
            let lens: Vec<f64> = (0..n)
                .map(|i| {
                    let (a, b) = (c[i], c[(i + 1) % n]);
                    let plain = (b[0] - a[0]).hypot(b[1] - a[1]);
                    if twisting {
                        plain
                    } else {
                        let scaled = ((b[0] - a[0]) * scale[0]).hypot((b[1] - a[1]) * scale[1]);
                        plain.max(scaled)
                    }
                })
                .collect();
            let counts = contour_segments(&lens, segments, frags, refining);
            if counts.iter().all(|&k| k <= 1) {
                return c.clone();
            }
            let mut out = Vec::with_capacity(counts.iter().map(|&k| k as usize).sum());
            for i in 0..c.len() {
                let (a, b) = (c[i], c[(i + 1) % c.len()]);
                for step in 0..counts[i] {
                    let t = step as f64 / counts[i] as f64;
                    out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
                }
            }
            out
        })
        .collect()
}

/// Layers a twisted extrusion is swept in when `slices=` is omitted, matching
/// OpenSCAD 2024.12.
///
/// Two independent limits, whichever is tighter: no slice may twist more than
/// `$fa` degrees, and no slice may move the outermost profile point further
/// than `$fs` along its helical path. `$fn`, when set, replaces both with a
/// plain "`$fn` slices per full revolution".
/// Layers a twisted and/or non-uniformly scaled extrusion is swept in when
/// `slices=` is omitted.
///
/// Twist and scale each impose their own count and the larger wins. For twist,
/// two limits whichever is tighter: no slice may turn more than `$fa` degrees,
/// and no slice may move the outermost point further than `$fs` along its
/// helical path. For a non-uniform scale, no slice may move a profile point
/// further than `$fs` along its straight path to the scaled position — `$fa`
/// plays no part, having no angle to bound. `$fn`, when set, replaces both with
/// a flat count.
fn implicit_slices(
    twist: f64,
    height: f64,
    rmax: f64,
    scale: Point2,
    max_scale_travel: f64,
    frags: FragmentSpec,
) -> u32 {
    let twisting = twist != 0.0;
    let scaling = non_uniform(scale);
    if !twisting && !scaling {
        return 1;
    }
    if frags.fn_ > 0.0 {
        // `$fn` slices per full revolution for a twist; a bare non-uniform
        // scale has no revolution, so it takes `$fn` outright.
        let by_twist = if twisting {
            (frags.fn_ * twist.abs() / 360.0).ceil()
        } else {
            0.0
        };
        let by_scale = if scaling { frags.fn_ } else { 0.0 };
        return (by_twist.max(by_scale) as u32).max(1);
    }
    let mut slices = 1.0f64;
    if twisting {
        let arc = rmax * twist.abs().to_radians();
        let helix = arc.hypot(height.abs());
        let by_angle = if frags.fa > 0.0 {
            (twist.abs() / frags.fa).ceil()
        } else {
            f64::INFINITY
        };
        let by_length = if frags.fs > 0.0 {
            (helix / frags.fs).ceil()
        } else {
            f64::INFINITY
        };
        slices = slices.max(by_angle.min(by_length));
    }
    if scaling && frags.fs > 0.0 {
        slices = slices.max((max_scale_travel.hypot(height.abs()) / frags.fs).ceil());
    }
    (slices as u32).max(1)
}

/// `linear_extrude` of the contours to a mesh, cutting out holes (even-odd) in
/// the caps and giving every contour (outer and hole) a wall loop.
#[allow(clippy::too_many_arguments)]
pub fn linear_extrude(
    contours: &[Contour],
    height: f64,
    center: bool,
    twist: f64,
    scale: Point2,
    slices: Option<u32>,
    segments: u32,
    frags: FragmentSpec,
) -> Mesh {
    // Twisting bends the walls, so the profile is resampled before the caps are
    // triangulated — OpenSCAD's caps carry the refined points too.
    let refined = refine_contours(contours, segments, frags, twist, scale);
    let slices = slices.unwrap_or_else(|| {
        let rmax = refined
            .iter()
            .flatten()
            .map(|p| p[0].hypot(p[1]))
            .fold(0.0f64, f64::max);
        // How far the worst-placed profile point travels to its scaled position.
        let travel = refined
            .iter()
            .flatten()
            .map(|p| (p[0] * (scale[0] - 1.0)).hypot(p[1] * (scale[1] - 1.0)))
            .fold(0.0f64, f64::max);
        implicit_slices(twist, height, rmax, scale, travel, frags)
    });
    let (points, ranges, cap_tris) = prepare(&refined);
    let mut mesh = Mesh::new();
    if points.is_empty() {
        return mesh;
    }
    let n = points.len();
    let slices = slices.max(1);
    let z0 = if center { -height / 2.0 } else { 0.0 };

    // `slices+1` rings of all points, twisted/scaled per layer.
    for layer in 0..=slices {
        let t = layer as f64 / slices as f64;
        let ang = (-twist * t).to_radians();
        let (s, c) = (ang.sin(), ang.cos());
        let sx = 1.0 + (scale[0] - 1.0) * t;
        let sy = 1.0 + (scale[1] - 1.0) * t;
        let z = z0 + height * t;
        for p in &points {
            let (x, y) = (p[0] * sx, p[1] * sy);
            mesh.verts.push([x * c - y * s, x * s + y * c, z]);
        }
    }
    let ring = |layer: u32, i: usize| layer * n as u32 + i as u32;

    // Walls: each contour range forms a loop at every layer.
    //
    // A twisted or non-uniformly scaled quad is not planar, so its two diagonals
    // enclose *different* volumes — on a 32-gon twisted by exactly one vertex
    // step, one reproduces the prism exactly and the other cuts ~1.3% off it.
    // OpenSCAD splits along the **shorter** diagonal, falling back to the wall's
    // lean only when the two are equal. Matched per quad against the oracle over
    // 3142 quads spanning positive and negative twist, off-axis profiles, pure
    // scale, and a 720-degree sweep.
    for &(start, len) in &ranges {
        let area2: f64 = (0..len)
            .map(|k| {
                let p = points[start + k];
                let q = points[start + (k + 1) % len];
                p[0] * q[1] - q[0] * p[1]
            })
            .sum();
        let ccw = area2 >= 0.0;
        // Ties are pervasive — any profile symmetric about the sweep leaves the
        // two diagonals exactly equal — and they break by which way the wall
        // leans: the twist direction, flipped for a hole because its indices run
        // the other way round.
        let lean_ac = (twist >= 0.0) == ccw;
        let dsq = |p: u32, q: u32| {
            let (p, q) = (mesh.verts[p as usize], mesh.verts[q as usize]);
            (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)
        };
        for layer in 0..slices {
            for k in 0..len {
                let i = start + k;
                let j = start + (k + 1) % len;
                let (a, b) = (ring(layer, i), ring(layer, j));
                let (cc, d) = (ring(layer + 1, j), ring(layer + 1, i));
                let (ac, bd) = (dsq(a, cc), dsq(b, d));
                let split_ac = if (ac - bd).abs() <= 1e-9 * ac.max(bd) {
                    lean_ac
                } else {
                    ac < bd
                };
                if split_ac {
                    mesh.tris.push([a, b, cc]);
                    mesh.tris.push([a, cc, d]);
                } else {
                    mesh.tris.push([a, b, d]);
                    mesh.tris.push([b, cc, d]);
                }
            }
        }
    }

    // Caps: bottom (reversed) + top, holes already removed by earcut.
    for t in &cap_tris {
        let (a, b, cc) = (t[0] as usize, t[1] as usize, t[2] as usize);
        mesh.tris.push([ring(0, a), ring(0, cc), ring(0, b)]);
        mesh.tris
            .push([ring(slices, a), ring(slices, b), ring(slices, cc)]);
    }

    mesh.ensure_outward();
    mesh
}

/// `rotate_extrude` of the contours around the Z axis.
pub fn rotate_extrude(contours: &[Contour], angle: f64, frags: FragmentSpec) -> Mesh {
    let mut mesh = Mesh::new();

    // OpenSCAD accepts a profile wholly on either side of the Y axis, but not
    // one that crosses it.  Treat the whole contour set as one profile: two
    // disjoint contours on opposite sides are invalid for the same reason as a
    // single crossing contour.  Points on the axis are allowed.
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    for p in contours.iter().flatten() {
        min_x = min_x.min(p[0]);
        max_x = max_x.max(p[0]);
    }
    if min_x < 0.0 && max_x > 0.0 {
        return mesh;
    }

    // Resolution is based on distance from the axis, not signed X.  Using the
    // signed maximum collapsed a wholly-negative profile to the minimum three
    // fragments even when `$fn` requested more.
    let max_r = min_x.abs().max(max_x.abs());

    // A revolution never covers more than one turn.  Preserve the sign (sweep
    // direction), but clamp larger magnitudes to a full revolution.  A zero
    // sweep is empty rather than a pair of coincident end caps.
    let sweep = angle.clamp(-360.0, 360.0);
    if sweep.abs() < 1e-12 {
        return mesh;
    }
    let full = (sweep.abs() - 360.0).abs() < 1e-9;
    let full_steps = fragments(max_r, frags).max(3);
    let steps = if full {
        full_steps
    } else {
        ((full_steps as f64 * sweep.abs() / 360.0).ceil() as u32).max(1)
    };
    for c in contours {
        revolve_one(&mut mesh, c, sweep, steps, full);
    }
    mesh.ensure_outward();
    mesh
}

fn revolve_one(mesh: &mut Mesh, contour: &[Point2], angle: f64, steps: u32, full: bool) {
    if contour.len() < 3 {
        return;
    }
    let owned: Vec<Point2>;
    let contour: &[Point2] = if signed_area(contour) < 0.0 {
        owned = contour.iter().rev().cloned().collect();
        &owned
    } else {
        contour
    };
    let n = contour.len();
    let base = mesh.verts.len() as u32;
    let ring_count = if full { steps } else { steps + 1 };
    for k in 0..ring_count {
        let frac = k as f64 / steps as f64;
        let th = (angle * frac).to_radians();
        let (s, c) = (th.sin(), th.cos());
        for p in contour {
            // 2D point (x=radius, y=height) -> 3D ring.
            mesh.verts.push([p[0] * c, p[0] * s, p[1]]);
        }
    }
    let ring = |k: u32, i: usize| base + (k % ring_count) * n as u32 + i as u32;
    // Walls span `steps` sectors whether or not the sweep is a full revolution
    // (the extra open-arc ring is a cap boundary, not another wall sector).
    let wall_steps = steps;
    for k in 0..wall_steps {
        for i in 0..n {
            let j = (i + 1) % n;
            let a = ring(k, i);
            let b = ring(k, j);
            let cc = ring(k + 1, j);
            let d = ring(k + 1, i);
            mesh.tris.push([a, b, cc]);
            mesh.tris.push([a, cc, d]);
        }
    }
    // End caps for a partial sweep.
    if !full {
        let tris = triangulate_simple(contour);
        for tri in &tris {
            mesh.tris
                .push([ring(0, tri[0]), ring(0, tri[2]), ring(0, tri[1])]);
            mesh.tris.push([
                ring(steps, tri[0]),
                ring(steps, tri[1]),
                ring(steps, tri[2]),
            ]);
        }
    }
}

#[cfg(test)]
mod rotate_extrude_tests {
    use super::*;

    fn rect(x0: f64, x1: f64) -> Contour {
        vec![[x0, 0.0], [x1, 0.0], [x1, 3.0], [x0, 3.0]]
    }

    fn spec(fn_: f64) -> FragmentSpec {
        FragmentSpec {
            fn_,
            ..FragmentSpec::default()
        }
    }

    #[test]
    fn rotate_extrude_negative_x_keeps_side_and_resolution() {
        let mesh = rotate_extrude(&[rect(-12.0, -10.0)], 90.0, spec(24.0));
        let (lo, hi) = mesh.bbox().expect("negative-X profile should revolve");

        // 90° of a 24-fragment circle is six sectors: 6 * 4 profile edges * 2
        // wall triangles, plus two triangles on each end cap.
        assert_eq!(mesh.tris.len(), 52);
        assert!((lo[0] + 12.0).abs() < 1e-9 && hi[0].abs() < 1e-9);
        assert!((lo[1] + 12.0).abs() < 1e-9 && hi[1].abs() < 1e-9);
    }

    #[test]
    fn rotate_extrude_rejects_profile_crossing_axis() {
        let mesh = rotate_extrude(&[rect(-1.0, 1.0)], 360.0, spec(24.0));
        assert!(mesh.is_empty());
    }

    #[test]
    fn rotate_extrude_partial_sweep_scales_fragments() {
        let mesh = rotate_extrude(&[rect(10.0, 12.0)], 90.0, spec(24.0));
        assert_eq!(mesh.tris.len(), 52);
        assert_eq!(mesh.verts.len(), 28); // seven rings of four profile points
    }

    #[test]
    fn rotate_extrude_clamps_sweep_to_one_turn() {
        let contour = rect(10.0, 12.0);
        let full = rotate_extrude(std::slice::from_ref(&contour), 360.0, spec(24.0));
        let over = rotate_extrude(&[contour], 450.0, spec(24.0));
        assert_eq!(over, full);
    }

    #[test]
    fn rotate_extrude_zero_sweep_is_empty() {
        assert!(rotate_extrude(&[rect(10.0, 12.0)], 0.0, spec(24.0)).is_empty());
    }
}

#[cfg(test)]
mod offset_tests {
    use super::*;

    fn square(s: f64) -> Contour {
        vec![[0.0, 0.0], [s, 0.0], [s, s], [0.0, s]]
    }
    /// Net filled area (outers positive, holes negative).
    fn area(cs: &[Contour]) -> f64 {
        cs.iter().map(|c| signed_area(c)).sum::<f64>().abs()
    }

    #[test]
    fn offset_grow_miter_square() {
        let r = offset(&[square(10.0)], 0.0, 2.0, false, FragmentSpec::default());
        assert!((area(&r) - 196.0).abs() < 1e-6, "grow area {}", area(&r));
    }

    #[test]
    fn offset_mild_inset_square() {
        let r = offset(&[square(10.0)], 0.0, -2.0, false, FragmentSpec::default());
        assert!((area(&r) - 36.0).abs() < 1e-6, "inset area {}", area(&r));
    }

    /// An inset larger than the shape must collapse to nothing, not a
    /// self-intersecting bowtie (the A3 bug).
    #[test]
    fn offset_over_inset_collapses_to_empty() {
        let r = offset(&[square(10.0)], 0.0, -10.0, false, FragmentSpec::default());
        assert!(r.is_empty(), "over-inset should be empty, got {r:?}");
    }

    /// Convex ⊕ convex stays exact: [0,10]² ⊕ [0,2]² = [0,12]² (area 144).
    #[test]
    fn minkowski_2d_convex_is_exact() {
        let r = minkowski_2d(vec![vec![square(10.0)], vec![square(2.0)]]);
        assert!(
            (area(&r) - 144.0).abs() < 1e-6,
            "square⊕square area {}",
            area(&r)
        );
    }

    /// A non-convex operand keeps its concavity: the L ⊕ square area is strictly
    /// below the convex-hull approximation (the A5 bug) and above the un-grown L.
    #[test]
    fn minkowski_2d_nonconvex_beats_convex_hull() {
        let l: Contour = vec![
            [0.0, 0.0],
            [24.0, 0.0],
            [24.0, 6.0],
            [6.0, 6.0],
            [6.0, 24.0],
            [0.0, 24.0],
        ];
        let got = area(&minkowski_2d(vec![vec![l.clone()], vec![square(2.0)]]));
        // Old convex approximation: hull of all pairwise vertex sums.
        let mut sums = Vec::new();
        for a in &l {
            for b in &square(2.0) {
                sums.push([a[0] + b[0], a[1] + b[1]]);
            }
        }
        let convex_area = area(&hull_2d(sums));
        assert!(got > 252.0, "should exceed the L's own area (252): {got}");
        assert!(
            got < convex_area - 10.0,
            "exact {got} not below convex approx {convex_area}"
        );
    }
}

#[cfg(test)]
mod extrude_refinement_tests {
    use super::*;

    fn spec(fn_: f64, fa: f64, fs: f64) -> FragmentSpec {
        FragmentSpec { fn_, fa, fs }
    }

    fn square(side: f64) -> Contour {
        vec![[0.0, 0.0], [side, 0.0], [side, side], [0.0, side]]
    }

    fn rect(w: f64, h: f64) -> Contour {
        vec![[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]]
    }

    /// Edge lengths of a closed contour, as `refine_contours` computes them for
    /// the twist case.
    fn lens(c: &[Point2]) -> Vec<f64> {
        (0..c.len())
            .map(|i| {
                let (a, b) = (c[i], c[(i + 1) % c.len()]);
                (b[0] - a[0]).hypot(b[1] - a[1])
            })
            .collect()
    }

    fn segs(c: &[Point2], segments: u32, frags: FragmentSpec, refining: bool) -> Vec<u32> {
        contour_segments(&lens(c), segments, frags, refining)
    }

    /// Every expectation below was read off OpenSCAD 2024.12.17 by exporting the
    /// mesh and counting the points on each profile edge.
    #[test]
    fn profile_budget_is_apportioned_by_edge_length() {
        // $fa=12 gives a budget of 30. A square's four edges each want exactly
        // 7.5, and the four-way tie for the last two segments goes unawarded —
        // 28, not 30.
        assert_eq!(
            segs(&square(10.0), 0, spec(0.0, 12.0, 0.01), true),
            vec![7, 7, 7, 7]
        );
        // Unequal edges split the same budget unevenly and do reach 30.
        assert_eq!(
            segs(&rect(10.0, 3.0), 0, spec(0.0, 12.0, 0.01), true),
            vec![12, 3, 12, 3]
        );
        // A short edge still takes one segment, and that shrinks the pool the
        // long edges draw from (14, not 15).
        assert_eq!(
            segs(&rect(100.0, 2.0), 0, spec(0.0, 12.0, 0.01), true),
            vec![14, 1, 14, 1]
        );
        // Halving $fa doubles the budget.
        assert_eq!(
            segs(&square(10.0), 0, spec(0.0, 6.0, 0.01), true),
            vec![15, 15, 15, 15]
        );
    }

    #[test]
    fn fs_caps_each_edge_and_fn_replaces_the_budget() {
        // $fs=2 over a 10-long edge allows 5, below the $fa budget of 7.
        assert_eq!(
            segs(&square(10.0), 0, spec(0.0, 12.0, 2.0), true),
            vec![5, 5, 5, 5]
        );
        // The cap is a ceiling, so 10/4 = 2.5 rounds up to 3.
        assert_eq!(
            segs(&square(10.0), 0, spec(0.0, 12.0, 4.0), true),
            vec![3, 3, 3, 3]
        );
        // $fn replaces the budget and disables the $fs cap.
        assert_eq!(
            segs(&square(10.0), 0, spec(8.0, 12.0, 4.0), true),
            vec![2, 2, 2, 2]
        );
        // A budget below the edge count still leaves one segment per edge.
        assert_eq!(
            segs(&square(10.0), 0, spec(3.0, 12.0, 2.0), true),
            vec![1, 1, 1, 1]
        );
    }

    #[test]
    fn segments_overrides_everything_and_applies_without_twist() {
        assert_eq!(
            segs(&square(10.0), 8, spec(40.0, 12.0, 2.0), true),
            vec![2, 2, 2, 2]
        );
        // No twist: `segments=` still refines, but `$fn` alone does not.
        assert_eq!(
            segs(&square(10.0), 8, spec(0.0, 12.0, 2.0), false),
            vec![2, 2, 2, 2]
        );
        assert_eq!(
            segs(&square(10.0), 0, spec(40.0, 12.0, 2.0), false),
            vec![1, 1, 1, 1]
        );
    }

    #[test]
    fn slices_take_the_tighter_of_the_angle_and_helix_limits() {
        // square(10)'s far corner
        let r = 10.0f64.hypot(10.0);
        // $fa=12 caps the twist per slice: ceil(90/12) = 8, tighter than the
        // helix limit of 13.
        assert_eq!(
            implicit_slices(90.0, 10.0, r, [1.0, 1.0], 0.0, spec(0.0, 12.0, 2.0)),
            8
        );
        // At $fa=6 the angle limit relaxes to 15 and the helix limit binds.
        assert_eq!(
            implicit_slices(90.0, 10.0, r, [1.0, 1.0], 0.0, spec(0.0, 6.0, 2.0)),
            13
        );
        assert_eq!(
            implicit_slices(360.0, 10.0, r, [1.0, 1.0], 0.0, spec(0.0, 6.0, 2.0)),
            45
        );
        assert_eq!(
            implicit_slices(720.0, 10.0, r, [1.0, 1.0], 0.0, spec(0.0, 6.0, 2.0)),
            89
        );
        // $fn is a flat count per full revolution, rounded up.
        assert_eq!(
            implicit_slices(90.0, 10.0, r, [1.0, 1.0], 0.0, spec(40.0, 12.0, 2.0)),
            10
        );
        assert_eq!(
            implicit_slices(30.0, 10.0, r, [1.0, 1.0], 0.0, spec(8.0, 12.0, 2.0)),
            1
        );
        // Sign of the twist does not change the count.
        assert_eq!(
            implicit_slices(-90.0, 10.0, r, [1.0, 1.0], 0.0, spec(0.0, 12.0, 2.0)),
            8
        );
        // No twist, no slicing.
        assert_eq!(
            implicit_slices(0.0, 10.0, r, [1.0, 1.0], 0.0, spec(0.0, 12.0, 2.0)),
            1
        );
    }

    /// A non-uniform scale bends the walls the way a twist does, so it refines
    /// too — measured against OpenSCAD 2024.12.17. The length that earns an edge
    /// its share is `max(original, scaled)`: under `scale=[1,2]` the y-aligned
    /// edges stretch to 20 and take twice the share of the x-aligned ones.
    #[test]
    fn non_uniform_scale_refines_by_the_stretched_edge_length() {
        let sq = square(10.0);
        let stretched: Vec<f64> = vec![10.0, 20.0, 10.0, 20.0];
        assert_eq!(
            contour_segments(&stretched, 0, spec(0.0, 12.0, 2.0), true),
            vec![5, 10, 5, 10]
        );
        // A uniform scale keeps the walls planar, so nothing is refined however
        // far from 1 it is.
        assert!(!non_uniform([0.5, 0.5]));
        assert!(!non_uniform([2.0, 2.0]));
        assert!(non_uniform([1.0, 2.0]));
        // With no twist and a uniform scale there is nothing to refine.
        assert_eq!(segs(&sq, 0, spec(0.0, 12.0, 2.0), false), vec![1, 1, 1, 1]);
    }

    #[test]
    fn non_uniform_scale_adds_slices_from_the_travel_distance() {
        let r = 10.0f64.hypot(10.0);
        // The far corner travels hypot(10*(1-0.2), 10*(1-2)) = 12.806, and with
        // height 10 that is a 16.25-long path: ceil(16.25/2) = 9.
        let travel = (10.0f64 * 0.8).hypot(10.0);
        assert_eq!(
            implicit_slices(0.0, 10.0, r, [0.2, 2.0], travel, spec(0.0, 12.0, 2.0)),
            9
        );
        // Height drives it too.
        assert_eq!(
            implicit_slices(0.0, 100.0, r, [0.2, 2.0], travel, spec(0.0, 12.0, 2.0)),
            51
        );
        // `$fa` has no angle to bound here, so only `$fs` matters.
        assert_eq!(
            implicit_slices(0.0, 10.0, r, [0.2, 2.0], travel, spec(0.0, 6.0, 2.0)),
            9
        );
        // `$fn` replaces the count outright.
        assert_eq!(
            implicit_slices(0.0, 10.0, r, [0.2, 2.0], travel, spec(8.0, 12.0, 2.0)),
            8
        );
        // Uniform scale adds none.
        assert_eq!(
            implicit_slices(0.0, 10.0, r, [0.5, 0.5], 0.0, spec(0.0, 12.0, 2.0)),
            1
        );
        // Twist and scale each propose a count; the larger wins.
        assert_eq!(
            implicit_slices(90.0, 10.0, r, [0.2, 2.0], travel, spec(0.0, 12.0, 2.0)),
            9
        );
    }

    /// The shorter diagonal is not merely a heuristic here: on a 32-gon twisted
    /// by exactly one vertex step per slice the solid is still a prism, and only
    /// one diagonal reproduces it. Both twist directions must land on it.
    #[test]
    fn twisted_walls_pick_the_shorter_diagonal() {
        // A 32-gon twisted by exactly one vertex step per slice is still a
        // prism; the wrong diagonal shaves ~1.3% off its volume.
        let n = 32;
        let poly: Contour = (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / n as f64;
                [5.0 * a.cos(), 5.0 * a.sin()]
            })
            .collect();
        let prism = 0.5 * n as f64 * 25.0 * (std::f64::consts::TAU / n as f64).sin() * 10.0;
        for twist in [90.0, -90.0] {
            let m = linear_extrude(
                std::slice::from_ref(&poly),
                10.0,
                false,
                twist,
                [1.0, 1.0],
                Some(8),
                0,
                spec(32.0, 12.0, 2.0),
            );
            let v = m.volume().abs();
            assert!(
                (v - prism).abs() < 1e-6,
                "twist {twist}: volume {v} should be the prism {prism}"
            );
        }
    }
}
