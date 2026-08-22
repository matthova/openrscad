//! Import and export of 2D vector formats (DXF today; SVG lives here too) as
//! even-odd `Contour` sets — the same representation `shape2d` uses for all 2D
//! geometry, so imported profiles flow straight into `linear_extrude`, booleans,
//! `offset`, etc.

use crate::shape2d::{chain_segments, Contour, Point2};
use crate::tessellate::fragments;
use openrscad_ir::FragmentSpec;
use std::f64::consts::PI;

/// Format a coordinate compactly (round-trip precision, no trailing-zero noise).
fn num(x: f64) -> String {
    let x = if x == 0.0 { 0.0 } else { x }; // normalize -0
    let mut s = format!("{x:.6}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    s
}

// ---------------------------------------------------------------------------
// DXF
// ---------------------------------------------------------------------------

/// One parsed entity: its type name (from group code 0) and its remaining
/// (group-code, value) properties in file order.
type Entity = (String, Vec<(i32, String)>);

/// A DXF file is a flat stream of (group-code, value) *line pairs*. Parse it
/// into entities, splitting on each `0` code (which introduces a new entity).
fn parse_entities(text: &str) -> Vec<Entity> {
    let mut lines = text.lines();
    let mut entities: Vec<Entity> = Vec::new();
    let mut cur: Option<Entity> = None;
    while let (Some(code_line), Some(val_line)) = (lines.next(), lines.next()) {
        let Ok(code) = code_line.trim().parse::<i32>() else {
            continue;
        };
        let val = val_line.trim().to_string();
        if code == 0 {
            if let Some(e) = cur.take() {
                entities.push(e);
            }
            cur = Some((val, Vec::new()));
        } else if let Some((_, props)) = cur.as_mut() {
            props.push((code, val));
        }
    }
    if let Some(e) = cur.take() {
        entities.push(e);
    }
    entities
}

fn first_f64(props: &[(i32, String)], code: i32) -> Option<f64> {
    props
        .iter()
        .find(|(c, _)| *c == code)
        .and_then(|(_, v)| v.parse().ok())
}

fn first_i32(props: &[(i32, String)], code: i32) -> Option<i32> {
    props
        .iter()
        .find(|(c, _)| *c == code)
        .and_then(|(_, v)| v.trim().parse::<f64>().ok())
        .map(|f| f as i32)
}

/// Tessellate a circle/arc sweep from `a0`..`a1` (radians) about `center`.
fn arc_points(
    center: Point2,
    r: f64,
    a0: f64,
    a1: f64,
    closed: bool,
    frags: FragmentSpec,
) -> Vec<Point2> {
    let n = fragments(r, frags).max(3);
    let sweep = a1 - a0;
    // For a full circle emit exactly `n` points (no duplicate closing vertex);
    // for an arc, emit `n` steps proportional to its span, endpoints included.
    let steps = if closed {
        n
    } else {
        ((n as f64) * (sweep.abs() / (2.0 * PI))).ceil().max(1.0) as u32
    };
    let count = if closed { steps } else { steps + 1 };
    (0..count)
        .map(|i| {
            // `count` differs for closed vs open arcs; the parameter step does not.
            let t = i as f64 / steps as f64;
            let a = a0 + sweep * t;
            [center[0] + r * a.cos(), center[1] + r * a.sin()]
        })
        .collect()
}

/// Parse an ASCII DXF into closed contours. Handles the entity types OpenSCAD
/// emits and reads: `LWPOLYLINE`, `POLYLINE`/`VERTEX`, `LINE`, `CIRCLE`, `ARC`.
/// Loose `LINE`/open-polyline segments are chained into loops the way OpenSCAD
/// stitches DXF line soup.
/// Polyline vertices, dropping the bulge factors.
///
/// A DXF bulge (group 42) denotes an arc between two vertices, and it would be
/// easy to expand into one — but OpenSCAD 2024.12 does not: a two-vertex closed
/// polyline bulged into a full circle imports as *nothing* there, for both
/// LWPOLYLINE and old-style POLYLINE/VERTEX. Expanding the arcs here would make
/// us disagree with the oracle rather than agree with it, so the factors are
/// read and discarded. `SPLINE` is skipped for the same measured reason.
fn drop_bulges(pts: &[(Point2, f64)]) -> Vec<Point2> {
    pts.iter().map(|(p, _)| *p).collect()
}

/// Import a DXF's 2D outlines.
///
/// `layer` keeps only entities on that layer (group 8); `None` takes every
/// entity, as an omitted `layer=` does upstream.
pub fn import_dxf(bytes: &[u8], layer: Option<&str>, frags: FragmentSpec) -> Vec<Contour> {
    let text = String::from_utf8_lossy(bytes);
    let mut entities = parse_entities(&text);
    if let Some(want) = layer {
        entities.retain(|(_, props)| {
            props
                .iter()
                .any(|(code, value)| *code == 8 && value.trim() == want)
        });
    }
    let mut contours: Vec<Contour> = Vec::new();
    let mut segs: Vec<(Point2, Point2)> = Vec::new();

    let mut i = 0;
    while i < entities.len() {
        let (ty, props) = &entities[i];
        match ty.as_str() {
            "LINE" => {
                let a = [
                    first_f64(props, 10).unwrap_or(0.0),
                    first_f64(props, 20).unwrap_or(0.0),
                ];
                let b = [
                    first_f64(props, 11).unwrap_or(0.0),
                    first_f64(props, 21).unwrap_or(0.0),
                ];
                segs.push((a, b));
            }
            "LWPOLYLINE" => {
                let closed = first_i32(props, 70).unwrap_or(0) & 1 != 0;
                // Group 42 is the *previous* vertex's bulge, so it is attached
                // to the point already pushed.
                let mut pts: Vec<(Point2, f64)> = Vec::new();
                let mut px: Option<f64> = None;
                for (c, v) in props {
                    match c {
                        10 => px = v.parse().ok(),
                        20 => {
                            if let (Some(x), Ok(y)) = (px.take(), v.parse::<f64>()) {
                                pts.push(([x, y], 0.0));
                            }
                        }
                        42 => {
                            if let (Some(last), Ok(b)) = (pts.last_mut(), v.parse::<f64>()) {
                                last.1 = b;
                            }
                        }
                        _ => {}
                    }
                }
                push_polyline(drop_bulges(&pts), closed, &mut contours, &mut segs);
            }
            "POLYLINE" => {
                let closed = first_i32(props, 70).unwrap_or(0) & 1 != 0;
                let mut pts: Vec<(Point2, f64)> = Vec::new();
                i += 1;
                while i < entities.len() && entities[i].0 == "VERTEX" {
                    let vp = &entities[i].1;
                    pts.push((
                        [
                            first_f64(vp, 10).unwrap_or(0.0),
                            first_f64(vp, 20).unwrap_or(0.0),
                        ],
                        first_f64(vp, 42).unwrap_or(0.0),
                    ));
                    i += 1;
                }
                if i < entities.len() && entities[i].0 == "SEQEND" {
                    i += 1;
                }
                push_polyline(drop_bulges(&pts), closed, &mut contours, &mut segs);
                continue;
            }
            "CIRCLE" => {
                let c = [
                    first_f64(props, 10).unwrap_or(0.0),
                    first_f64(props, 20).unwrap_or(0.0),
                ];
                let r = first_f64(props, 40).unwrap_or(0.0);
                if r > 0.0 {
                    contours.push(arc_points(c, r, 0.0, 2.0 * PI, true, frags));
                }
            }
            "ARC" => {
                let c = [
                    first_f64(props, 10).unwrap_or(0.0),
                    first_f64(props, 20).unwrap_or(0.0),
                ];
                let r = first_f64(props, 40).unwrap_or(0.0);
                let a0 = first_f64(props, 50).unwrap_or(0.0).to_radians();
                let mut a1 = first_f64(props, 51).unwrap_or(0.0).to_radians();
                if a1 <= a0 {
                    a1 += 2.0 * PI; // DXF arcs go CCW
                }
                if r > 0.0 {
                    let pts = arc_points(c, r, a0, a1, false, frags);
                    for w in pts.windows(2) {
                        segs.push((w[0], w[1]));
                    }
                }
            }
            "ELLIPSE" => {
                let c = [
                    first_f64(props, 10).unwrap_or(0.0),
                    first_f64(props, 20).unwrap_or(0.0),
                ];
                // Group 11/21 is the major-axis endpoint *relative* to the
                // centre, so it carries both the semi-major length and the
                // rotation; 40 is the minor/major ratio.
                let major = [
                    first_f64(props, 11).unwrap_or(0.0),
                    first_f64(props, 21).unwrap_or(0.0),
                ];
                let ratio = first_f64(props, 40).unwrap_or(1.0);
                let t0 = first_f64(props, 41).unwrap_or(0.0);
                let t1 = first_f64(props, 42).unwrap_or(2.0 * PI);
                let a = major[0].hypot(major[1]);
                let b = a * ratio;
                if a > 0.0 {
                    let rot = major[1].atan2(major[0]);
                    let (sr, cr) = rot.sin_cos();
                    let full = (t1 - t0).abs() >= 2.0 * PI - 1e-9;
                    let n = fragments(a.max(b), frags).max(3);
                    let steps = if full {
                        n
                    } else {
                        ((n as f64) * ((t1 - t0).abs() / (2.0 * PI)))
                            .ceil()
                            .max(1.0) as u32
                    };
                    let count = if full { steps } else { steps + 1 };
                    let pts: Vec<Point2> = (0..count)
                        .map(|k| {
                            let t = t0 + (t1 - t0) * (k as f64 / steps as f64);
                            let (x, y) = (a * t.cos(), b * t.sin());
                            [c[0] + x * cr - y * sr, c[1] + x * sr + y * cr]
                        })
                        .collect();
                    if full {
                        contours.push(pts);
                    } else {
                        for w in pts.windows(2) {
                            segs.push((w[0], w[1]));
                        }
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }

    contours.extend(chain_segments(segs));
    contours
}

/// A closed polyline is a contour outright; an open one contributes segments to
/// the stitching pool.
fn push_polyline(
    pts: Vec<Point2>,
    closed: bool,
    contours: &mut Vec<Contour>,
    segs: &mut Vec<(Point2, Point2)>,
) {
    if closed && pts.len() >= 3 {
        contours.push(pts);
    } else {
        for w in pts.windows(2) {
            segs.push((w[0], w[1]));
        }
    }
}

/// Serialize contours as an ASCII DXF (`ENTITIES` of closed `LWPOLYLINE`s), the
/// shape OpenSCAD both emits and re-imports.
pub fn export_dxf(contours: &[Contour]) -> String {
    let mut s = String::from("999\nDXF from OpenRSCAD\n  0\nSECTION\n  2\nENTITIES\n");
    for c in contours {
        if c.len() < 2 {
            continue;
        }
        s.push_str("  0\nLWPOLYLINE\n  8\n0\n 90\n");
        s.push_str(&format!("{}\n 70\n1\n", c.len()));
        for p in c {
            s.push_str(&format!(" 10\n{}\n 20\n{}\n", num(p[0]), num(p[1])));
        }
    }
    s.push_str("  0\nENDSEC\n  0\nEOF\n");
    s
}

// ---------------------------------------------------------------------------
// SVG
// ---------------------------------------------------------------------------

/// Steps used to flatten a Bézier segment or a circle/ellipse.
const CURVE_STEPS: usize = 24;

/// Parse a length like `100`, `10mm`, `2.5in` into millimetres. Bare numbers
/// and `px` are treated as pixels at 72 DPI — matching OpenSCAD 2024.12.
fn svg_len(s: &str) -> Option<f64> {
    let s = s.trim();
    let split = s
        .find(|c: char| {
            !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E')
        })
        .unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let v: f64 = num.parse().ok()?;
    let f = match unit.trim() {
        "mm" => 1.0,
        "cm" => 10.0,
        "in" => 25.4,
        "pt" => 25.4 / 72.0,
        "pc" => 25.4 / 6.0,
        "" | "px" => 25.4 / 72.0,
        _ => 25.4 / 72.0,
    };
    Some(v * f)
}

/// Extract an XML attribute value (`name="..."` or `name='...'`) from a tag body.
fn attr(tag: &str, name: &str) -> Option<String> {
    let mut from = 0;
    while let Some(rel) = tag[from..].find(name) {
        let i = from + rel;
        // Must be a standalone attribute name (preceded by whitespace/tag start).
        let before_ok = i == 0 || tag.as_bytes()[i - 1].is_ascii_whitespace();
        let rest = tag[i + name.len()..].trim_start();
        if before_ok && rest.starts_with('=') {
            let rest = rest[1..].trim_start();
            let q = rest.chars().next()?;
            if q == '"' || q == '\'' {
                let end = rest[1..].find(q)?;
                return Some(rest[1..1 + end].to_string());
            }
        }
        from = i + name.len();
    }
    None
}

fn attr_f64(tag: &str, name: &str) -> Option<f64> {
    attr(tag, name).and_then(|v| v.trim().parse().ok())
}

/// Scan every floating-point number out of a coordinate string (handles commas,
/// whitespace, and sign/scientific notation glued to the previous number).
fn scan_numbers(s: &str) -> Vec<f64> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_ascii_digit() || c == '.' || c == '-' || c == '+' {
            let start = i;
            if c == '-' || c == '+' {
                i += 1;
            }
            let mut seen_dot = false;
            while i < b.len() {
                let d = b[i] as char;
                if d.is_ascii_digit() {
                    i += 1;
                } else if d == '.' && !seen_dot {
                    seen_dot = true;
                    i += 1;
                } else if (d == 'e' || d == 'E') && i + 1 < b.len() {
                    i += 1;
                    if (b[i] as char) == '-' || (b[i] as char) == '+' {
                        i += 1;
                    }
                } else {
                    break;
                }
            }
            if let Ok(v) = s[start..i].parse::<f64>() {
                out.push(v);
            }
        } else {
            i += 1;
        }
    }
    out
}

fn points_attr(tag: &str) -> Vec<Point2> {
    let nums = attr(tag, "points")
        .map(|s| scan_numbers(&s))
        .unwrap_or_default();
    nums.chunks_exact(2).map(|c| [c[0], c[1]]).collect()
}

fn cubic(p0: Point2, p1: Point2, p2: Point2, p3: Point2, out: &mut Vec<Point2>) {
    for i in 1..=CURVE_STEPS {
        let t = i as f64 / CURVE_STEPS as f64;
        let u = 1.0 - t;
        let x = u * u * u * p0[0]
            + 3.0 * u * u * t * p1[0]
            + 3.0 * u * t * t * p2[0]
            + t * t * t * p3[0];
        let y = u * u * u * p0[1]
            + 3.0 * u * u * t * p1[1]
            + 3.0 * u * t * t * p2[1]
            + t * t * t * p3[1];
        out.push([x, y]);
    }
}

fn quad(p0: Point2, p1: Point2, p2: Point2, out: &mut Vec<Point2>) {
    for i in 1..=CURVE_STEPS {
        let t = i as f64 / CURVE_STEPS as f64;
        let u = 1.0 - t;
        out.push([
            u * u * p0[0] + 2.0 * u * t * p1[0] + t * t * p2[0],
            u * u * p0[1] + 2.0 * u * t * p1[1] + t * t * p2[1],
        ]);
    }
}

/// Flatten an SVG elliptical-arc (`A`) command from `p0` to `p1`.
#[allow(clippy::too_many_arguments)]
fn svg_arc(
    p0: Point2,
    mut rx: f64,
    mut ry: f64,
    phi: f64,
    laf: bool,
    sf: bool,
    p1: Point2,
    out: &mut Vec<Point2>,
) {
    if rx == 0.0 || ry == 0.0 || (p0[0] == p1[0] && p0[1] == p1[1]) {
        out.push(p1);
        return;
    }
    rx = rx.abs();
    ry = ry.abs();
    let (cosp, sinp) = (phi.cos(), phi.sin());
    let dx = (p0[0] - p1[0]) / 2.0;
    let dy = (p0[1] - p1[1]) / 2.0;
    let x1p = cosp * dx + sinp * dy;
    let y1p = -sinp * dx + cosp * dy;
    let mut lam = x1p * x1p / (rx * rx) + y1p * y1p / (ry * ry);
    if lam > 1.0 {
        let s = lam.sqrt();
        rx *= s;
        ry *= s;
        lam = 1.0;
    }
    let sign = if laf != sf { 1.0 } else { -1.0 };
    let num = (rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p).max(0.0);
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let co = sign * (num / den).sqrt();
    let cxp = co * rx * y1p / ry;
    let cyp = -co * ry * x1p / rx;
    let cx = cosp * cxp - sinp * cyp + (p0[0] + p1[0]) / 2.0;
    let cy = sinp * cxp + cosp * cyp + (p0[1] + p1[1]) / 2.0;
    let ang = |ux: f64, uy: f64, vx: f64, vy: f64| {
        let dot = ux * vx + uy * vy;
        let len = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        let mut a = (dot / len).clamp(-1.0, 1.0).acos();
        if ux * vy - uy * vx < 0.0 {
            a = -a;
        }
        a
    };
    let theta1 = ang(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut dtheta = ang(
        (x1p - cxp) / rx,
        (y1p - cyp) / ry,
        (-x1p - cxp) / rx,
        (-y1p - cyp) / ry,
    );
    if !sf && dtheta > 0.0 {
        dtheta -= 2.0 * PI;
    } else if sf && dtheta < 0.0 {
        dtheta += 2.0 * PI;
    }
    for i in 1..=CURVE_STEPS {
        let t = theta1 + dtheta * (i as f64 / CURVE_STEPS as f64);
        let x = cosp * rx * t.cos() - sinp * ry * t.sin() + cx;
        let y = sinp * rx * t.cos() + cosp * ry * t.sin() + cy;
        out.push([x, y]);
    }
    let _ = lam;
}

/// A cursor over a path `d` string that yields command letters and numbers.
struct PathCursor<'a> {
    b: &'a [u8],
    s: &'a str,
    i: usize,
}

impl<'a> PathCursor<'a> {
    fn new(s: &'a str) -> Self {
        PathCursor {
            b: s.as_bytes(),
            s,
            i: 0,
        }
    }
    fn skip_sep(&mut self) {
        while self.i < self.b.len() {
            let c = self.b[self.i] as char;
            if c.is_ascii_whitespace() || c == ',' {
                self.i += 1;
            } else {
                break;
            }
        }
    }
    /// Peek the next command letter, if the next non-separator byte is one.
    fn peek_cmd(&mut self) -> Option<char> {
        self.skip_sep();
        let c = *self.b.get(self.i)? as char;
        c.is_ascii_alphabetic().then_some(c)
    }
    fn take_cmd(&mut self) -> Option<char> {
        let c = self.peek_cmd()?;
        self.i += 1;
        Some(c)
    }
    /// Read one number (skipping leading separators). Returns None if the next
    /// token isn't numeric.
    fn num(&mut self) -> Option<f64> {
        self.skip_sep();
        let start = self.i;
        let c = *self.b.get(self.i)? as char;
        if !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+') {
            return None;
        }
        if c == '-' || c == '+' {
            self.i += 1;
        }
        let mut seen_dot = false;
        while self.i < self.b.len() {
            let d = self.b[self.i] as char;
            if d.is_ascii_digit() {
                self.i += 1;
            } else if d == '.' && !seen_dot {
                seen_dot = true;
                self.i += 1;
            } else if (d == 'e' || d == 'E') && self.i + 1 < self.b.len() {
                self.i += 1;
                if (self.b[self.i] as char) == '-' || (self.b[self.i] as char) == '+' {
                    self.i += 1;
                }
            } else {
                break;
            }
        }
        self.s[start..self.i].parse().ok()
    }
    fn pair(&mut self) -> Option<Point2> {
        Some([self.num()?, self.num()?])
    }
}

/// Parse an SVG path `d` string into (closed contours, open polylines). Supports
/// M/L/H/V/C/S/Q/T/A/Z (absolute + relative), with repeated argument groups.
fn parse_path(d: &str) -> (Vec<Contour>, Vec<Vec<Point2>>) {
    let mut closed = Vec::new();
    let mut open = Vec::new();
    let mut sub: Vec<Point2> = Vec::new();
    let (mut cx, mut cy) = (0.0f64, 0.0f64);
    let (mut sx, mut sy) = (0.0f64, 0.0f64);
    let mut prev_c: Option<Point2> = None; // last cubic control (for S)
    let mut prev_q: Option<Point2> = None; // last quad control (for T)
    let mut c = PathCursor::new(d);
    // Each outer iteration reads one command letter, then the inner loop consumes
    // all of its (possibly repeated) argument groups until the next letter.
    while let Some(cmd) = c.take_cmd() {
        if cmd == 'Z' || cmd == 'z' {
            if sub.len() >= 3 {
                closed.push(std::mem::take(&mut sub));
            } else {
                sub.clear(); // degenerate subpath
            }
            cx = sx;
            cy = sy;
            prev_c = None;
            prev_q = None;
            continue;
        }
        let rel = cmd.is_ascii_lowercase();
        let is_curve = matches!(cmd, 'C' | 'c' | 'S' | 's' | 'Q' | 'q' | 'T' | 't');
        let mut up = cmd.to_ascii_uppercase();
        // Emit at least one group, then keep consuming groups until the next
        // token is a command letter or the numbers run out.
        loop {
            match up {
                'M' => {
                    let Some(p) = c.pair() else { break };
                    if !sub.is_empty() {
                        open.push(std::mem::take(&mut sub));
                    }
                    cx = if rel { cx + p[0] } else { p[0] };
                    cy = if rel { cy + p[1] } else { p[1] };
                    sx = cx;
                    sy = cy;
                    sub.push([cx, cy]);
                    up = 'L'; // subsequent implicit pairs are lineto
                    if c.peek_cmd().is_some() {
                        break;
                    }
                    continue;
                }
                'L' => {
                    let Some(p) = c.pair() else { break };
                    cx = if rel { cx + p[0] } else { p[0] };
                    cy = if rel { cy + p[1] } else { p[1] };
                    sub.push([cx, cy]);
                }
                'H' => {
                    let Some(x) = c.num() else { break };
                    cx = if rel { cx + x } else { x };
                    sub.push([cx, cy]);
                }
                'V' => {
                    let Some(y) = c.num() else { break };
                    cy = if rel { cy + y } else { y };
                    sub.push([cx, cy]);
                }
                'C' => {
                    let (Some(a), Some(b), Some(e)) = (c.pair(), c.pair(), c.pair()) else {
                        break;
                    };
                    let p1 = [
                        if rel { cx + a[0] } else { a[0] },
                        if rel { cy + a[1] } else { a[1] },
                    ];
                    let p2 = [
                        if rel { cx + b[0] } else { b[0] },
                        if rel { cy + b[1] } else { b[1] },
                    ];
                    let p3 = [
                        if rel { cx + e[0] } else { e[0] },
                        if rel { cy + e[1] } else { e[1] },
                    ];
                    cubic([cx, cy], p1, p2, p3, &mut sub);
                    prev_c = Some(p2);
                    cx = p3[0];
                    cy = p3[1];
                }
                'S' => {
                    let (Some(b), Some(e)) = (c.pair(), c.pair()) else {
                        break;
                    };
                    let p1 = match prev_c {
                        Some(pc) => [2.0 * cx - pc[0], 2.0 * cy - pc[1]],
                        None => [cx, cy],
                    };
                    let p2 = [
                        if rel { cx + b[0] } else { b[0] },
                        if rel { cy + b[1] } else { b[1] },
                    ];
                    let p3 = [
                        if rel { cx + e[0] } else { e[0] },
                        if rel { cy + e[1] } else { e[1] },
                    ];
                    cubic([cx, cy], p1, p2, p3, &mut sub);
                    prev_c = Some(p2);
                    cx = p3[0];
                    cy = p3[1];
                }
                'Q' => {
                    let (Some(a), Some(e)) = (c.pair(), c.pair()) else {
                        break;
                    };
                    let p1 = [
                        if rel { cx + a[0] } else { a[0] },
                        if rel { cy + a[1] } else { a[1] },
                    ];
                    let p2 = [
                        if rel { cx + e[0] } else { e[0] },
                        if rel { cy + e[1] } else { e[1] },
                    ];
                    quad([cx, cy], p1, p2, &mut sub);
                    prev_q = Some(p1);
                    cx = p2[0];
                    cy = p2[1];
                }
                'T' => {
                    let Some(e) = c.pair() else { break };
                    let p1 = match prev_q {
                        Some(pq) => [2.0 * cx - pq[0], 2.0 * cy - pq[1]],
                        None => [cx, cy],
                    };
                    let p2 = [
                        if rel { cx + e[0] } else { e[0] },
                        if rel { cy + e[1] } else { e[1] },
                    ];
                    quad([cx, cy], p1, p2, &mut sub);
                    prev_q = Some(p1);
                    cx = p2[0];
                    cy = p2[1];
                }
                'A' => {
                    let (Some(rr), Some(rot), Some(f), Some(e)) =
                        (c.pair(), c.num(), c.pair(), c.pair())
                    else {
                        break;
                    };
                    let end = [
                        if rel { cx + e[0] } else { e[0] },
                        if rel { cy + e[1] } else { e[1] },
                    ];
                    svg_arc(
                        [cx, cy],
                        rr[0],
                        rr[1],
                        rot.to_radians(),
                        f[0] != 0.0,
                        f[1] != 0.0,
                        end,
                        &mut sub,
                    );
                    cx = end[0];
                    cy = end[1];
                }
                _ => break,
            }
            if !is_curve {
                prev_c = None;
            }
            if !matches!(up, 'Q' | 'T') {
                prev_q = None;
            }
            // Stop this group-run if the next token is a command letter.
            if c.peek_cmd().is_some() {
                break;
            }
        }
    }
    if !sub.is_empty() {
        open.push(sub);
    }
    (closed, open)
}

/// Parse an SVG document into contours, applying OpenSCAD's coordinate mapping
/// (72-DPI lengths, Y flipped about the physical height).
/// An SVG 2x3 affine transform, `[a b c d e f]` in the spec's ordering:
/// `x' = a·x + c·y + e`, `y' = b·x + d·y + f`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Affine([f64; 6]);

impl Affine {
    const IDENTITY: Affine = Affine([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    fn apply(&self, p: Point2) -> Point2 {
        let [a, b, c, d, e, f] = self.0;
        [a * p[0] + c * p[1] + e, b * p[0] + d * p[1] + f]
    }

    /// `self` then `rhs` — i.e. the child's transform composed under the
    /// parent's, which is how a nested `<g>` accumulates.
    fn then(&self, rhs: &Affine) -> Affine {
        let [a1, b1, c1, d1, e1, f1] = self.0;
        let [a2, b2, c2, d2, e2, f2] = rhs.0;
        Affine([
            a1 * a2 + c1 * b2,
            b1 * a2 + d1 * b2,
            a1 * c2 + c1 * d2,
            b1 * c2 + d1 * d2,
            a1 * e2 + c1 * f2 + e1,
            b1 * e2 + d1 * f2 + f1,
        ])
    }

    /// Parse a `transform=` attribute: a whitespace/comma separated list of
    /// `translate`, `scale`, `rotate`, `matrix`, `skewX` and `skewY`, applied
    /// left to right.
    fn parse(spec: &str) -> Affine {
        let mut out = Affine::IDENTITY;
        let mut rest = spec;
        while let Some(open) = rest.find('(') {
            let name = rest[..open].trim_matches(|c: char| !c.is_ascii_alphabetic());
            let Some(close) = rest[open..].find(')') else {
                break;
            };
            let args = scan_numbers(&rest[open + 1..open + close]);
            let g = |i: usize| args.get(i).copied();
            let step = match name {
                "translate" => {
                    Affine([1.0, 0.0, 0.0, 1.0, g(0).unwrap_or(0.0), g(1).unwrap_or(0.0)])
                }
                // A single argument scales both axes.
                "scale" => {
                    let sx = g(0).unwrap_or(1.0);
                    Affine([sx, 0.0, 0.0, g(1).unwrap_or(sx), 0.0, 0.0])
                }
                "rotate" => {
                    let (s, c) = g(0).unwrap_or(0.0).to_radians().sin_cos();
                    let rot = Affine([c, s, -s, c, 0.0, 0.0]);
                    // The 3-argument form rotates about a point.
                    match (g(1), g(2)) {
                        (Some(cx), Some(cy)) => Affine([1.0, 0.0, 0.0, 1.0, cx, cy])
                            .then(&rot)
                            .then(&Affine([1.0, 0.0, 0.0, 1.0, -cx, -cy])),
                        _ => rot,
                    }
                }
                "matrix" => Affine([
                    g(0).unwrap_or(1.0),
                    g(1).unwrap_or(0.0),
                    g(2).unwrap_or(0.0),
                    g(3).unwrap_or(1.0),
                    g(4).unwrap_or(0.0),
                    g(5).unwrap_or(0.0),
                ]),
                "skewX" => Affine([
                    1.0,
                    0.0,
                    g(0).unwrap_or(0.0).to_radians().tan(),
                    1.0,
                    0.0,
                    0.0,
                ]),
                "skewY" => Affine([
                    1.0,
                    g(0).unwrap_or(0.0).to_radians().tan(),
                    0.0,
                    1.0,
                    0.0,
                    0.0,
                ]),
                _ => Affine::IDENTITY,
            };
            out = out.then(&step);
            rest = &rest[open + close + 1..];
        }
        out
    }
}

/// Whether an element is hidden, as an attribute or inside `style=`.
///
/// Only `display:none` counts. Upstream renders a `visibility:hidden` element
/// regardless — measured, not assumed — so honouring it here would drop
/// geometry OpenSCAD keeps.
fn hidden(tag: &str) -> bool {
    let style = attr(tag, "style").unwrap_or_default().replace(' ', "");
    let display = attr(tag, "display").unwrap_or_default();
    display.trim() == "none" || style.contains("display:none")
}

/// A drawable element paired with the transform accumulated from its ancestors.
type Placed = (&'static str, String, Affine);

/// Walk the document, tracking the transform stack and skipping hidden
/// subtrees, and yield every drawable element with its accumulated transform.
///
/// This replaces a flat tag scan: without the hierarchy a `<g transform=…>` and
/// a `display:none` group both go unnoticed, and `<use>` cannot be resolved at
/// all.
fn walk_shapes(text: &str, select: Option<(Option<&str>, Option<&str>)>) -> Vec<Placed> {
    const SHAPES: [&str; 7] = [
        "rect", "circle", "ellipse", "line", "polyline", "polygon", "path",
    ];
    let mut out: Vec<Placed> = Vec::new();
    // (transform in effect, depth at which it was pushed)
    let mut stack: Vec<(Affine, usize)> = vec![(Affine::IDENTITY, 0)];
    // Depth of the shallowest hidden container, if we are inside one. Only a
    // container can set this: a self-closing element has no close tag to clear
    // it at, so it hides only itself.
    let mut hidden_at: Option<usize> = None;
    // `<defs>` is tracked apart from hiding because an explicit `id=` overrides
    // `display:none` but *not* membership of `<defs>` — upstream renders the
    // former and not the latter.
    let mut defs_at: Option<usize> = None;
    // Depth of the selected element, once found. Selection has to happen during
    // the walk, not by slicing the subtree out first: a shape picked by `id=`
    // still inherits every `transform=` from its ancestors, and cutting it out
    // of the text would drop them.
    let mut selected_at: Option<usize> = None;
    let mut depth = 0usize;
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // `<` and `>` are ASCII, so a match is always a char boundary; the
        // guard keeps the slice below safe on arbitrary bytes regardless.
        if bytes[i] != b'<' || !text.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let Some(rel_end) = text[i..].find('>') else {
            break;
        };
        let end = i + rel_end;
        let inner = &text[i + 1..end];
        let closing = inner.starts_with('/');
        let self_closing = inner.trim_end().ends_with('/');
        let name: String = inner
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        let lname = name.to_ascii_lowercase();

        if closing {
            depth = depth.saturating_sub(1);
            if stack.last().map(|(_, d)| *d) == Some(depth) && stack.len() > 1 {
                stack.pop();
            }
            if hidden_at == Some(depth) {
                hidden_at = None;
            }
            if defs_at == Some(depth) {
                defs_at = None;
            }
            if selected_at == Some(depth) {
                // Past the selected subtree: nothing further can match.
                break;
            }
            i = end + 1;
            continue;
        }

        let ctm = stack.last().map(|(t, _)| *t).unwrap_or(Affine::IDENTITY);
        let here = ctm.then(&Affine::parse(
            &attr(inner, "transform").unwrap_or_default(),
        ));

        // `<defs>` holds templates for `<use>` and draws nothing itself.
        let self_hidden = hidden(inner);
        if !self_closing {
            if hidden_at.is_none() && self_hidden {
                hidden_at = Some(depth);
            }
            if defs_at.is_none() && lname == "defs" {
                defs_at = Some(depth);
            }
        }
        // A self-closing match has no close tag to end the selection at, so
        // note it here and stop once this element has been emitted.
        let mut last_of_selection = false;
        if let Some((want_layer, want_id)) = select {
            if selected_at.is_none() {
                let hit = want_layer
                    .is_some_and(|w| attr(inner, "inkscape:label").as_deref() == Some(w))
                    || want_id.is_some_and(|w| attr(inner, "id").as_deref() == Some(w));
                if hit {
                    selected_at = Some(depth);
                    last_of_selection = self_closing;
                    // Asking for an element by name overrides `display:none`,
                    // its own and its ancestors' — upstream renders a hidden
                    // rect when you select it explicitly. It does not override
                    // `<defs>`, which stays unrenderable.
                    hidden_at = None;
                }
            }
        }
        let in_selection = select.is_none() || selected_at.is_some();
        // A self-closing element hides only itself, so its own flag is checked
        // here rather than entering the hidden state above.
        let own_ok = !(self_closing && self_hidden) || selected_at == Some(depth);
        let visible = hidden_at.is_none() && defs_at.is_none() && in_selection && own_ok;

        if visible {
            if let Some(shape) = SHAPES.iter().find(|s| **s == lname) {
                out.push((shape, inner.to_string(), here));
            } else if lname == "use" {
                // `<use href="#id">` instantiates another element, offset by
                // this element's own x/y.
                let href = attr(inner, "href")
                    .or_else(|| attr(inner, "xlink:href"))
                    .unwrap_or_default();
                let target = href.trim_start_matches('#');
                let offset = Affine([
                    1.0,
                    0.0,
                    0.0,
                    1.0,
                    attr_f64(inner, "x").unwrap_or(0.0),
                    attr_f64(inner, "y").unwrap_or(0.0),
                ]);
                if !target.is_empty() {
                    if let Some(sub) = svg_subtree(text, None, Some(target)) {
                        // Guard against a `<use>` that names itself.
                        if !sub.contains("<use") || sub.len() < text.len() {
                            let placed = here.then(&offset);
                            for (n, b, t) in walk_shapes(&sub, None) {
                                out.push((n, b, placed.then(&t)));
                            }
                        }
                    }
                }
            }
        }

        if last_of_selection {
            break;
        }
        if !self_closing && !SHAPES.iter().any(|s| *s == lname) && lname != "use" {
            // A container: its transform stays in effect until the close tag.
            stack.push((here, depth));
            depth += 1;
        } else if !self_closing && SHAPES.iter().any(|s| *s == lname) {
            // A shape written with a separate close tag still nests.
            depth += 1;
        }
        i = end + 1;
    }
    out
}

/// The text span of the element whose `id` (or Inkscape layer label) matches,
/// including its children.
///
/// Selection is done on the source text rather than in the element walk below,
/// which is flat: pulling the subtree out first keeps `layer=`/`id=` working
/// without the walk having to track nesting (still open as A-I08).
fn svg_subtree(text: &str, layer: Option<&str>, id: Option<&str>) -> Option<String> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i <= text.len() && text.is_char_boundary(i) {
        let Some(rel) = text[i..].find('<') else {
            break;
        };
        let start = i + rel;
        let Some(rel_end) = text[start..].find('>') else {
            break;
        };
        let end = start + rel_end;
        let tag = &text[start + 1..end];
        let name: String = tag
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        let matches = layer
            .map(|w| attr(tag, "inkscape:label").as_deref() == Some(w))
            .unwrap_or(false)
            || id
                .map(|w| attr(tag, "id").as_deref() == Some(w))
                .unwrap_or(false);
        if matches && !name.is_empty() {
            // Self-closing, or a leaf shape: the tag itself is the subtree.
            if tag.trim_end().ends_with('/') {
                return Some(text[start..=end].to_string());
            }
            // Otherwise scan to the matching close, allowing for nesting.
            let close = format!("</{name}");
            let open = format!("<{name}");
            let (mut depth, mut j) = (1usize, end + 1);
            while j < bytes.len() {
                // Slicing at a byte that is not a char boundary panics, and the
                // bytes here are arbitrary user input (found by fuzzing).
                if !text.is_char_boundary(j) {
                    j += 1;
                    continue;
                }
                if text[j..].starts_with(&close) {
                    depth -= 1;
                    if depth == 0 {
                        let stop = text[j..].find('>').map(|k| j + k).unwrap_or(j);
                        return Some(text[start..=stop].to_string());
                    }
                } else if text[j..].starts_with(&open) {
                    depth += 1;
                }
                j += 1;
            }
            return Some(text[start..].to_string());
        }
        i = end + 1;
    }
    None
}

/// Import an SVG's 2D outlines.
///
/// `layer` selects an Inkscape layer group by its label and `id` selects any
/// element by its `id`; either keeps that subtree alone. A selector that
/// matches nothing yields no geometry, as upstream.
pub fn import_svg(bytes: &[u8], layer: Option<&str>, id: Option<&str>, dpi: f64) -> Vec<Contour> {
    let text = String::from_utf8_lossy(bytes);
    let select = (layer.is_some() || id.is_some()).then_some((layer, id));
    // Locate the <svg ...> opening tag for viewBox/width/height.
    let svg_tag = find_tag(&text, "svg").unwrap_or_default();
    let vb: Vec<f64> = attr(&svg_tag, "viewBox")
        .map(|s| scan_numbers(&s))
        .unwrap_or_default();
    let (vb_min_x, vb_min_y, vb_w, vb_h) = if vb.len() == 4 {
        (vb[0], vb[1], vb[2], vb[3])
    } else {
        (0.0, 0.0, 0.0, 0.0)
    };
    let has_vb = vb.len() == 4;
    let width_mm = attr(&svg_tag, "width").and_then(|s| svg_len(&s));
    let height_mm = attr(&svg_tag, "height").and_then(|s| svg_len(&s));
    // A physical width pins the scale. Failing that, viewBox units are inches
    // at `dpi` — but only when there *is* a viewBox: with no viewBox at all
    // upstream takes the coordinates 1:1, whatever `dpi` says.
    let per_unit = if dpi > 0.0 { 25.4 / dpi } else { 25.4 / 72.0 };
    let scale = if has_vb {
        width_mm.map(|w| w / vb_w).unwrap_or(per_unit)
    } else {
        1.0
    };

    // Collect raw (SVG-coordinate) closed contours and open polylines.
    let mut raw_closed: Vec<Contour> = Vec::new();
    let mut raw_open: Vec<Vec<Point2>> = Vec::new();
    for (name, body, ctm) in walk_shapes(&text, select) {
        // Every shape is emitted in its own user space, so the accumulated
        // transform is applied here, before the viewBox mapping and Y flip.
        let put_closed = |dst: &mut Vec<Contour>, pts: Vec<Point2>| {
            dst.push(pts.into_iter().map(|p| ctm.apply(p)).collect());
        };
        let put_open = |dst: &mut Vec<Vec<Point2>>, pts: Vec<Point2>| {
            dst.push(pts.into_iter().map(|p| ctm.apply(p)).collect());
        };
        match name {
            "rect" => {
                let x = attr_f64(&body, "x").unwrap_or(0.0);
                let y = attr_f64(&body, "y").unwrap_or(0.0);
                let w = attr_f64(&body, "width").unwrap_or(0.0);
                let h = attr_f64(&body, "height").unwrap_or(0.0);
                if w > 0.0 && h > 0.0 {
                    put_closed(
                        &mut raw_closed,
                        vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
                    );
                }
            }
            "circle" => {
                let cx = attr_f64(&body, "cx").unwrap_or(0.0);
                let cy = attr_f64(&body, "cy").unwrap_or(0.0);
                let r = attr_f64(&body, "r").unwrap_or(0.0);
                if r > 0.0 {
                    put_closed(&mut raw_closed, ellipse_pts(cx, cy, r, r));
                }
            }
            "ellipse" => {
                let cx = attr_f64(&body, "cx").unwrap_or(0.0);
                let cy = attr_f64(&body, "cy").unwrap_or(0.0);
                let rx = attr_f64(&body, "rx").unwrap_or(0.0);
                let ry = attr_f64(&body, "ry").unwrap_or(0.0);
                if rx > 0.0 && ry > 0.0 {
                    put_closed(&mut raw_closed, ellipse_pts(cx, cy, rx, ry));
                }
            }
            "line" => {
                let p0 = [
                    attr_f64(&body, "x1").unwrap_or(0.0),
                    attr_f64(&body, "y1").unwrap_or(0.0),
                ];
                let p1 = [
                    attr_f64(&body, "x2").unwrap_or(0.0),
                    attr_f64(&body, "y2").unwrap_or(0.0),
                ];
                put_open(&mut raw_open, vec![p0, p1]);
            }
            "polyline" => put_open(&mut raw_open, points_attr(&body)),
            "polygon" => {
                let pts = points_attr(&body);
                if pts.len() >= 3 {
                    put_closed(&mut raw_closed, pts);
                }
            }
            "path" => {
                if let Some(d) = attr(&body, "d") {
                    let (c, o) = parse_path(&d);
                    for pts in c {
                        put_closed(&mut raw_closed, pts);
                    }
                    for pts in o {
                        put_open(&mut raw_open, pts);
                    }
                }
            }
            _ => {}
        }
    }

    // Physical height for the Y flip (falls back to the content bbox).
    let content_max_y = raw_closed
        .iter()
        .flatten()
        .chain(raw_open.iter().flatten())
        .map(|p| p[1])
        .fold(f64::MIN, f64::max);
    let phys_h = height_mm.unwrap_or(if has_vb {
        vb_h * scale
    } else {
        content_max_y.max(0.0)
    });

    let map = |p: &Point2| {
        [
            (p[0] - vb_min_x) * scale,
            phys_h - (p[1] - vb_min_y) * scale,
        ]
    };
    let mut contours: Vec<Contour> = raw_closed
        .iter()
        .map(|c| c.iter().map(map).collect())
        .collect();
    // Stitch open polylines/lines into loops (rare, but matches DXF behaviour).
    let mut segs: Vec<(Point2, Point2)> = Vec::new();
    for poly in &raw_open {
        let mapped: Vec<Point2> = poly.iter().map(map).collect();
        for w in mapped.windows(2) {
            segs.push((w[0], w[1]));
        }
    }
    contours.extend(chain_segments(segs));
    contours
}

fn ellipse_pts(cx: f64, cy: f64, rx: f64, ry: f64) -> Contour {
    let n = fragments(rx.max(ry), FragmentSpec::default()).max(3);
    (0..n)
        .map(|i| {
            let a = 2.0 * PI * i as f64 / n as f64;
            [cx + rx * a.cos(), cy + ry * a.sin()]
        })
        .collect()
}

/// Find the first `<name ...>` opening tag and return its full text.
fn find_tag(text: &str, name: &str) -> Option<String> {
    let needle = format!("<{name}");
    let start = text.find(&needle)?;
    let end = text[start..].find('>')? + start;
    Some(text[start..=end].to_string())
}

/// Serialize contours as an SVG `<path>` (Y negated, so it re-imports upright),
/// mirroring OpenSCAD's exporter (`width`/`height` in mm, `viewBox` = neg bbox).
pub fn export_svg(contours: &[Contour]) -> String {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in contours.iter().flatten() {
        min_x = min_x.min(p[0]);
        min_y = min_y.min(p[1]);
        max_x = max_x.max(p[0]);
        max_y = max_y.max(p[1]);
    }
    if contours.is_empty() {
        (min_x, min_y, max_x, max_y) = (0.0, 0.0, 0.0, 0.0);
    }
    let w = max_x - min_x;
    let h = max_y - min_y;
    let mut d = String::new();
    for c in contours {
        if c.len() < 2 {
            continue;
        }
        for (k, p) in c.iter().enumerate() {
            d.push_str(if k == 0 { "M " } else { "L " });
            d.push_str(&format!("{},{} ", num(p[0]), num(-p[1])));
        }
        d.push_str("z ");
    }
    format!(
        "<?xml version=\"1.0\" standalone=\"no\"?>\n\
         <svg width=\"{w}mm\" height=\"{h}mm\" viewBox=\"{vx} {vy} {w} {h}\" \
         xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\">\n\
         <path d=\"{d}\" stroke=\"black\" fill=\"lightgray\" stroke-width=\"0.5\"/>\n\
         </svg>\n",
        w = num(w),
        h = num(h),
        vx = num(min_x),
        vy = num(-max_y),
        d = d.trim(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sum of signed contour areas (shoelace), even-odd — positive outers,
    /// negative holes give the net filled area.
    fn net_area(cs: &[Contour]) -> f64 {
        cs.iter()
            .map(|c| {
                let mut a = 0.0;
                for i in 0..c.len() {
                    let p = c[i];
                    let q = c[(i + 1) % c.len()];
                    a += p[0] * q[1] - q[0] * p[1];
                }
                a / 2.0
            })
            .sum()
    }

    #[test]
    fn dxf_roundtrip_square_with_hole() {
        // Outer 10x20 CCW, inner 2x2 hole CW -> net area 200 - 4 = 196.
        let outer = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 20.0], [0.0, 20.0]];
        let hole = vec![[4.0, 4.0], [4.0, 6.0], [6.0, 6.0], [6.0, 4.0]];
        let dxf = export_dxf(&[outer, hole]);
        let back = import_dxf(dxf.as_bytes(), None, FragmentSpec::default());
        assert_eq!(back.len(), 2, "expected two contours");
        assert!(
            (net_area(&back).abs() - 196.0).abs() < 1e-6,
            "area {}",
            net_area(&back).abs()
        );
    }

    #[test]
    fn dxf_imports_line_soup() {
        // Four LINE entities forming a closed 5x5 square get stitched into a loop.
        let dxf = "\
0\nSECTION\n2\nENTITIES\n\
0\nLINE\n10\n0\n20\n0\n11\n5\n21\n0\n\
0\nLINE\n10\n5\n20\n0\n11\n5\n21\n5\n\
0\nLINE\n10\n5\n20\n5\n11\n0\n21\n5\n\
0\nLINE\n10\n0\n20\n5\n11\n0\n21\n0\n\
0\nENDSEC\n0\nEOF\n";
        let cs = import_dxf(dxf.as_bytes(), None, FragmentSpec::default());
        assert_eq!(cs.len(), 1);
        assert!((net_area(&cs).abs() - 25.0).abs() < 1e-6);
    }

    #[test]
    fn svg_rect_scale_and_yflip() {
        // No viewBox: coords 1:1, Y flipped about height*25.4/72.
        let svg =
            r#"<svg width="100" height="100"><rect x="10" y="20" width="40" height="30"/></svg>"#;
        let cs = import_svg(svg.as_bytes(), None, None, 72.0);
        assert_eq!(cs.len(), 1);
        let xs: Vec<f64> = cs[0].iter().map(|p| p[0]).collect();
        let ys: Vec<f64> = cs[0].iter().map(|p| p[1]).collect();
        let (minx, maxx) = (
            xs.iter().cloned().fold(f64::MAX, f64::min),
            xs.iter().cloned().fold(f64::MIN, f64::max),
        );
        let (miny, maxy) = (
            ys.iter().cloned().fold(f64::MAX, f64::min),
            ys.iter().cloned().fold(f64::MIN, f64::max),
        );
        assert!(
            (minx - 10.0).abs() < 1e-6 && (maxx - 50.0).abs() < 1e-6,
            "x {minx}..{maxx}"
        );
        // Y: flip axis = 100*25.4/72 = 35.2778; rect y 20..50 -> 15.278..-14.722
        assert!(
            (maxy - 15.2778).abs() < 1e-3 && (miny + 14.7222).abs() < 1e-3,
            "y {miny}..{maxy}"
        );
        assert!((net_area(&cs).abs() - 1200.0).abs() < 1e-6);
    }

    #[test]
    fn svg_path_relative_and_close() {
        // A unit square via relative lineto, closed with Z.
        let svg = r#"<svg viewBox="0 0 10 10" width="10mm" height="10mm"><path d="M1,1 l4,0 l0,4 l-4,0 z"/></svg>"#;
        let cs = import_svg(svg.as_bytes(), None, None, 72.0);
        assert_eq!(cs.len(), 1);
        assert!(
            (net_area(&cs).abs() - 16.0).abs() < 1e-6,
            "area {}",
            net_area(&cs).abs()
        );
    }

    #[test]
    fn svg_roundtrip_preserves_area() {
        let sq = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 20.0], [0.0, 20.0]];
        let svg = export_svg(&[sq]);
        let back = import_svg(svg.as_bytes(), None, None, 72.0);
        assert!(
            (net_area(&back).abs() - 200.0).abs() < 1e-6,
            "area {}",
            net_area(&back).abs()
        );
    }
}

#[cfg(test)]
mod robustness_tests {
    use super::*;

    /// Both importers take user-supplied bytes straight from `import()`, so
    /// neither may panic on rubbish. The tree walk added for transforms scans
    /// byte by byte and slices as it goes, which is exactly how a multi-byte
    /// character gets cut in half — this covers that class directly, since
    /// `cargo fuzz` needs a nightly toolchain and does not run in `cargo test`.
    #[test]
    fn importers_survive_arbitrary_bytes() {
        let multibyte = "<svg><g transform=\"translate(1,2)\"><rect id=\"日本語テキスト\"                          width=\"4\" height=\"4\"/></g></svg>";
        let mut cases: Vec<Vec<u8>> = vec![
            Vec::new(),
            b"<".to_vec(),
            b"<svg".to_vec(),
            b"<svg><g transform=\"".to_vec(),
            b"<svg><g transform=\"rotate(\"".to_vec(),
            b"<svg><use href=\"#a\" id=\"a\"/></svg>".to_vec(), // self-reference
            b"<svg><g><g><g><rect/>".to_vec(),                  // unclosed nesting
            multibyte.as_bytes().to_vec(),
            vec![0xff, 0xfe, 0x3c, 0x73, 0x76, 0x67, 0x3e], // invalid UTF-8 lead
        ];
        // Every truncation of the multi-byte document, which walks the cut
        // through the middle of each character.
        for n in 0..multibyte.len() {
            cases.push(multibyte.as_bytes()[..n].to_vec());
        }
        // And the same bytes with a lone continuation byte spliced in.
        for n in (0..multibyte.len()).step_by(7) {
            let mut v = multibyte.as_bytes()[..n].to_vec();
            v.push(0x9f);
            v.extend_from_slice(&multibyte.as_bytes()[n..]);
            cases.push(v);
        }
        for bytes in &cases {
            let _ = import_svg(bytes, None, None, 72.0);
            let _ = import_svg(bytes, Some("L1"), None, 72.0);
            let _ = import_svg(bytes, None, Some("a"), 72.0);
            let _ = import_dxf(bytes, None, FragmentSpec::default());
            let _ = import_dxf(bytes, Some("0"), FragmentSpec::default());
        }
    }
}
