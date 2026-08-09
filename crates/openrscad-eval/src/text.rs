//! `text()` — turn a string into 2D glyph outlines (contours). Fonts live in a
//! shared [`fontdb`] database: the bundled Liberation family (Sans/Serif/Mono ×
//! Regular/Bold/Italic/BoldItalic, SIL OFL — the exact files OpenSCAD ships) is
//! *always* loaded, so `font=` on those families selects the same face and glyph
//! shapes match OpenSCAD byte-for-byte, native and in the browser alike. Hosts
//! may additionally register OS fonts ([`register_system_fonts`], native) or raw
//! font bytes ([`register_font_data`], the browser's Local Font Access blobs) so
//! `text(font="…")` can use system fonts too. Outlines come from `ttf-parser`;
//! Bézier segments are flattened to line segments.
//!
//! The result is a set of contours (outer boundaries and holes) that become a
//! `Node::Polygon`; even-odd triangulation in `openrscad-geom` turns them into a
//! filled 2D region (with holes) that can be rendered or extruded.
//!
//! The database is a process-wide singleton so the parts that can't thread host
//! config through (the evaluator's `text()` handler) still see registered fonts.
//! It starts with *only* the bundled faces, keeping plain `cargo test` and the
//! geometry oracle deterministic — system fonts appear only once a host opts in.

use std::sync::{OnceLock, RwLock};
use ttf_parser::Face;

use fontdb::{Database, Family, Query, Stretch, Style, Weight};

/// The family used when `font=` is empty or the requested family isn't found.
const DEFAULT_FAMILY: &str = "Liberation Sans";

/// The bundled Liberation faces (SIL Open Font License; the exact Liberation
/// 2.00.1 files OpenSCAD ships). Always loaded into the database so glyphs match
/// OpenSCAD byte-for-byte everywhere. See `fonts/LICENSE`.
static BUNDLED_FILES: &[&[u8]] = &[
    include_bytes!("../fonts/LiberationSans-Regular.ttf"),
    include_bytes!("../fonts/LiberationSans-Bold.ttf"),
    include_bytes!("../fonts/LiberationSans-Italic.ttf"),
    include_bytes!("../fonts/LiberationSans-BoldItalic.ttf"),
    include_bytes!("../fonts/LiberationSerif-Regular.ttf"),
    include_bytes!("../fonts/LiberationSerif-Bold.ttf"),
    include_bytes!("../fonts/LiberationSerif-Italic.ttf"),
    include_bytes!("../fonts/LiberationSerif-BoldItalic.ttf"),
    include_bytes!("../fonts/LiberationMono-Regular.ttf"),
    include_bytes!("../fonts/LiberationMono-Bold.ttf"),
    include_bytes!("../fonts/LiberationMono-Italic.ttf"),
    include_bytes!("../fonts/LiberationMono-BoldItalic.ttf"),
];

/// The process-wide font database, seeded with the bundled Liberation faces.
fn db() -> &'static RwLock<Database> {
    static DB: OnceLock<RwLock<Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = Database::new();
        for &bytes in BUNDLED_FILES {
            db.load_font_data(bytes.to_vec());
        }
        // Empty `font=` maps to the sans-serif default; make that Liberation Sans.
        db.set_sans_serif_family(DEFAULT_FAMILY);
        RwLock::new(db)
    })
}

/// Load the operating system's installed fonts into the shared database so
/// `text(font="…")` and [`font_completions`] can use them. Native only (the
/// browser has no filesystem — see [`register_font_data`]). Idempotent: the
/// first call scans, later calls are no-ops. Hosts that want reproducible,
/// machine-independent output (e.g. the geometry oracle) simply never call it.
pub fn register_system_fonts() {
    // Filesystem font loading is native-only (the `fs` fontdb feature); on wasm
    // this is a no-op — the browser supplies fonts via [`register_font_data`].
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        static LOADED: AtomicBool = AtomicBool::new(false);
        if LOADED.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Ok(mut db) = db().write() {
            db.load_system_fonts();
        }
    }
}

/// Register a font from raw file bytes (a `.ttf`/`.otf`/`.ttc`), making all of
/// its faces available to `text(font="…")` and [`font_completions`]. This is how
/// the browser supplies fonts obtained via the Local Font Access API. Identical
/// files passed more than once are loaded only once. Returns the number of faces
/// newly added.
pub fn register_font_data(bytes: Vec<u8>) -> usize {
    // Dedup identical files: the browser hands us one blob per *face*, but a
    // collection (`.ttc`) yields the same bytes for every face it contains, and
    // the same model may be re-rendered many times. A content hash keeps us from
    // reloading (and re-parsing) the same file over and over.
    static SEEN: OnceLock<RwLock<rustc_hash::FxHashSet<u64>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| RwLock::new(rustc_hash::FxHashSet::default()));
    let hash = fnv1a(&bytes);
    if let Ok(mut s) = seen.write() {
        if !s.insert(hash) {
            return 0;
        }
    }
    let Ok(mut db) = db().write() else {
        return 0;
    };
    let before = db.len();
    db.load_font_data(bytes);
    db.len().saturating_sub(before)
}

/// FNV-1a hash of a byte slice, for cheap file dedup in [`register_font_data`].
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Parse an OpenSCAD `font` string (`"Family"` or `"Family:style=Style"`) into a
/// family name and the weight/slant the style requests. An empty family means
/// the default (Liberation Sans). Matching is case-insensitive; the style's
/// spaces are ignored, and `bold`/`italic` may appear in either order.
fn parse_font(font: &str) -> (String, Weight, Style) {
    let (family_part, attrs) = font.split_once(':').unwrap_or((font, ""));
    let family = family_part.trim().to_string();
    let style = attrs
        .split(':')
        .find_map(|a| a.trim().strip_prefix("style="))
        .map(|s| s.trim().to_ascii_lowercase().replace(' ', ""))
        .unwrap_or_default();
    let weight = if style.contains("bold") {
        Weight::BOLD
    } else {
        Weight::NORMAL
    };
    let slant = if style.contains("italic") {
        Style::Italic
    } else if style.contains("oblique") {
        Style::Oblique
    } else {
        Style::Normal
    };
    (family, weight, slant)
}

/// Resolve a `font` string against the shared database and run `f` with the
/// matched [`Face`] and whether the requested *family* exists. An unknown family
/// falls back to Liberation Sans and reports `false` so the caller can warn; an
/// unavailable *style* within a known family silently uses the closest face
/// fontdb offers. The bundled Liberation Sans is always present, so a face is
/// always produced.
fn with_face<T>(font: &str, f: impl FnOnce(&Face, bool) -> T) -> T {
    let (family, weight, slant) = parse_font(font);
    let db = db().read().expect("font db lock");

    let requested = if family.is_empty() {
        DEFAULT_FAMILY
    } else {
        &family
    };
    let known = family.is_empty()
        || db.faces().any(|fi| {
            fi.families
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case(requested))
        });

    let query = |fam: &str| {
        db.query(&Query {
            families: &[Family::Name(fam)],
            weight,
            stretch: Stretch::Normal,
            style: slant,
        })
    };
    let id = query(requested)
        .or_else(|| query(DEFAULT_FAMILY))
        .expect("bundled Liberation Sans is always available");

    db.with_face_data(id, |data, index| {
        let face = Face::parse(data, index).expect("registered font parses");
        f(&face, known)
    })
    .expect("resolved font id is valid")
}

/// The coarse OpenSCAD `:style=` bucket for a fontdb face, chosen so a value from
/// [`font_completions`] resolves back (via [`with_face`]) to the same bucket.
fn style_label(weight: Weight, style: Style) -> &'static str {
    let bold = weight.0 >= Weight::BOLD.0;
    let italic = !matches!(style, Style::Normal);
    match (bold, italic) {
        (true, true) => "Bold Italic",
        (true, false) => "Bold",
        (false, true) => "Italic",
        (false, false) => "Regular",
    }
}

/// One `font=` value offered for editor autocompletion — see
/// [`font_completions`].
pub struct FontCompletion {
    /// The string to insert between the quotes, e.g. `Liberation Sans` or
    /// `Liberation Sans:style=Bold`.
    pub value: String,
    /// Human-readable family + style, e.g. `Liberation Sans — Bold`.
    pub detail: String,
}

/// The `font=` values to offer as editor autocompletions: every face currently
/// in the shared database (bundled Liberation plus any [`register_system_fonts`]
/// / [`register_font_data`] additions), as the `Family` string (for the regular
/// style) or the `Family:style=Style` string OpenSCAD uses. Deduped by
/// family+style bucket and sorted for stable ordering.
pub fn font_completions() -> Vec<FontCompletion> {
    let db = db().read().expect("font db lock");
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for fi in db.faces() {
        let Some((family, _)) = fi.families.first() else {
            continue;
        };
        let style = style_label(fi.weight, fi.style);
        if !seen.insert((family.to_ascii_lowercase(), style)) {
            continue;
        }
        let value = if style == "Regular" {
            family.clone()
        } else {
            format!("{family}:style={style}")
        };
        out.push(FontCompletion {
            value,
            detail: format!("{family} — {style}"),
        });
    }
    out.sort_by(|a, b| a.value.cmp(&b.value));
    out
}

/// Parameters for a `text()` call, minus the font (resolved by [`render_text`]).
pub struct TextParams<'a> {
    pub text: &'a str,
    pub size: f64,
    pub halign: &'a str,
    pub valign: &'a str,
    pub spacing: f64,
    pub direction: &'a str,
    /// Segments per Bézier curve (from `$fn`, clamped).
    pub segments: usize,
}

/// Build the glyph contours for `text(font=…, …)` as `(points, paths, known)`.
/// `known` is whether the requested family exists (see [`with_face`]); `false`
/// means the caller should warn that it fell back to Liberation Sans.
pub fn render_text(font: &str, params: &TextParams) -> (Vec<[f64; 2]>, Vec<Vec<u32>>, bool) {
    with_face(font, |face, known| {
        let (points, paths) = text_contours(&TextOpts {
            text: params.text,
            face,
            size: params.size,
            halign: params.halign,
            valign: params.valign,
            spacing: params.spacing,
            direction: params.direction,
            segments: params.segments,
        });
        (points, paths, known)
    })
}

/// Parameters for [`text_contours`]: a [`TextParams`] plus the resolved face.
struct TextOpts<'a> {
    text: &'a str,
    /// The resolved font face.
    face: &'a Face<'a>,
    size: f64,
    halign: &'a str,
    valign: &'a str,
    spacing: f64,
    direction: &'a str,
    /// Segments per Bézier curve (from `$fn`, clamped).
    segments: usize,
}

/// Flattens a glyph's outline into contours (in font units).
struct Outliner {
    contours: Vec<Vec<[f64; 2]>>,
    cur: Vec<[f64; 2]>,
    last: [f64; 2],
    seg: usize,
}

impl Outliner {
    fn new(seg: usize) -> Self {
        Outliner {
            contours: Vec::new(),
            cur: Vec::new(),
            last: [0.0, 0.0],
            seg: seg.max(1),
        }
    }
    fn flush(&mut self) {
        if self.cur.len() >= 2 {
            self.contours.push(std::mem::take(&mut self.cur));
        } else {
            self.cur.clear();
        }
    }
}

impl ttf_parser::OutlineBuilder for Outliner {
    fn move_to(&mut self, x: f32, y: f32) {
        self.flush();
        self.last = [x as f64, y as f64];
        self.cur.push(self.last);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.last = [x as f64, y as f64];
        self.cur.push(self.last);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (p0, c, p1) = (self.last, [x1 as f64, y1 as f64], [x as f64, y as f64]);
        for i in 1..=self.seg {
            let t = i as f64 / self.seg as f64;
            let u = 1.0 - t;
            self.cur.push([
                u * u * p0[0] + 2.0 * u * t * c[0] + t * t * p1[0],
                u * u * p0[1] + 2.0 * u * t * c[1] + t * t * p1[1],
            ]);
        }
        self.last = p1;
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (p0, c1, c2, p1) = (
            self.last,
            [x1 as f64, y1 as f64],
            [x2 as f64, y2 as f64],
            [x as f64, y as f64],
        );
        for i in 1..=self.seg {
            let t = i as f64 / self.seg as f64;
            let u = 1.0 - t;
            let (a, b, cc, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.cur.push([
                a * p0[0] + b * c1[0] + cc * c2[0] + d * p1[0],
                a * p0[1] + b * c1[1] + cc * c2[1] + d * p1[1],
            ]);
        }
        self.last = p1;
    }
    fn close(&mut self) {
        self.flush();
    }
}

/// Build the glyph contours for `opts` as `(points, paths)` suitable for a
/// `Node::Polygon`. Coordinates are in mm; the baseline is at y=0 for
/// `valign="baseline"`.
fn text_contours(opts: &TextOpts) -> (Vec<[f64; 2]>, Vec<Vec<u32>>) {
    let face = opts.face;
    let upem = face.units_per_em() as f64;
    if upem <= 0.0 {
        return (Vec::new(), Vec::new());
    }
    // OpenSCAD renders glyphs 100/72 larger than the nominal `size` (a FreeType
    // 72-DPI vs 100-unit-per-point convention); match it so text is the same
    // size as OpenSCAD's.
    let scale = opts.size / upem * (100.0 / 72.0);

    let chars: Vec<char> = opts.text.chars().collect();
    let advance = |c: char| -> f64 {
        face.glyph_index(c)
            .and_then(|g| face.glyph_hor_advance(g))
            .map(|a| a as f64 * scale * opts.spacing)
            .unwrap_or(0.0)
    };
    let widths: Vec<f64> = chars.iter().map(|&c| advance(c)).collect();
    let total: f64 = widths.iter().sum();

    let x0 = match opts.halign {
        "center" => -total / 2.0,
        "right" => -total,
        _ => 0.0,
    };
    let asc = face.ascender() as f64 * scale;
    let desc = face.descender() as f64 * scale; // negative
    let y0 = match opts.valign {
        "top" => -asc,
        "bottom" => -desc,
        "center" => -(asc + desc) / 2.0,
        _ => 0.0, // baseline
    };

    // Right-to-left just reverses the placement order.
    let rtl = opts.direction == "rtl";
    let order: Vec<usize> = if rtl {
        (0..chars.len()).rev().collect()
    } else {
        (0..chars.len()).collect()
    };

    let mut points: Vec<[f64; 2]> = Vec::new();
    let mut paths: Vec<Vec<u32>> = Vec::new();
    let mut pen_x = x0;

    for &i in &order {
        let c = chars[i];
        if let Some(gid) = face.glyph_index(c) {
            let mut o = Outliner::new(opts.segments);
            if face.outline_glyph(gid, &mut o).is_some() {
                o.flush();
                for contour in &o.contours {
                    if contour.len() < 3 {
                        continue;
                    }
                    let start = points.len() as u32;
                    for p in contour {
                        points.push([p[0] * scale + pen_x, p[1] * scale + y0]);
                    }
                    paths.push((start..points.len() as u32).collect());
                }
            }
        }
        pen_x += widths[i];
    }
    (points, paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Horizontal advance of `c` in the face `font` resolves to.
    fn advance(font: &str, c: char) -> u16 {
        with_face(font, |f, _| {
            f.glyph_hor_advance(f.glyph_index(c).unwrap()).unwrap()
        })
    }
    /// Whether the requested family exists in the (bundled-only, in tests) db.
    fn known(font: &str) -> bool {
        with_face(font, |_, k| k)
    }

    #[test]
    fn resolve_font_selects_family_and_reports_unknown() {
        // The db starts with only the bundled Liberation faces (tests never call
        // register_system_fonts), so this is deterministic. A known family
        // resolves and reports availability; an unknown one falls back to
        // Liberation Sans and reports `false` so the caller can warn.
        assert!(known("Liberation Serif"));
        assert!(known("Liberation Mono"));
        assert!(known("")); // empty == default Liberation Sans
        assert!(!known("Arial"));

        // Mono is fixed-width; Sans is proportional — proves distinct faces.
        assert_eq!(
            advance("Liberation Mono", 'i'),
            advance("Liberation Mono", 'M')
        );
        assert_ne!(
            advance("Liberation Sans", 'i'),
            advance("Liberation Sans", 'M')
        );
    }

    #[test]
    fn resolve_font_style_matching_is_case_and_space_insensitive() {
        // "Bold Italic", "bolditalic", mixed case all select the same face.
        let a = advance("Liberation Sans:style=Bold Italic", 'A');
        let b = advance("liberation sans:style=bolditalic", 'A');
        let regular = advance("Liberation Sans", 'A');
        assert_eq!(a, b);
        // The styled face is genuinely different from regular.
        assert_ne!(a, regular);
    }

    #[test]
    fn register_font_data_dedups_identical_files() {
        // The browser hands us one blob per face, so a collection (or a re-render)
        // re-sends the same bytes; the content-hash guard must load a given file
        // at most once. (We reuse a bundled TTF as a convenient real font file.)
        let bytes = BUNDLED_FILES[0].to_vec();
        let _ = register_font_data(bytes.clone());
        assert_eq!(
            register_font_data(bytes),
            0,
            "an identical file must not be reloaded"
        );
    }

    #[test]
    fn font_completions_include_bundled_families_and_styles() {
        let values: Vec<String> = font_completions().into_iter().map(|c| c.value).collect();
        // Regular is the bare family; other styles use the `:style=` form.
        assert!(values.iter().any(|v| v == "Liberation Sans"));
        assert!(values
            .iter()
            .any(|v| v == "Liberation Mono:style=Bold Italic"));
        // Every completion value round-trips: it names a known family.
        assert!(values.iter().all(|v| known(v)));
    }
}
