//! `textmetrics()` / `fontmetrics()` and the object values they return.
//!
//! Expected numbers are OpenSCAD 2024.12's own output (`--enable=textmetrics`)
//! for the bundled Liberation faces. `fontmetrics` matches to <1e-3 (it is a
//! straight scaling of the font's own tables); `textmetrics` matches to
//! <0.01mm — the residual is OpenSCAD's FreeType fixed-point outline scaling,
//! which we deliberately do not replicate (see COMPAT.md). Field names,
//! object echo format, alignment behaviour, and access forms are exact.

use openrscad_eval::eval_program;
use openrscad_syntax::parse;

fn echoes(src: &str) -> Vec<String> {
    eval_program(&parse(src).unwrap()).unwrap().echoes
}

/// The payload after the `ECHO: ` prefix.
fn payload(line: &str) -> &str {
    line.strip_prefix("ECHO: ").expect("echo line")
}

fn num(line: &str) -> f64 {
    payload(line).parse().expect("scalar echo")
}

fn vec2(line: &str) -> [f64; 2] {
    let s = payload(line).trim_start_matches('[').trim_end_matches(']');
    let mut it = s.split(", ").map(|x| x.parse::<f64>().unwrap());
    [it.next().unwrap(), it.next().unwrap()]
}

/// Within OpenSCAD's FreeType-scaling residual (see module docs).
#[track_caller]
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 0.02, "{a} not close to oracle {b}");
}

#[test]
fn textmetrics_matches_oracle_hello() {
    let e = echoes(
        r#"m = textmetrics("Hello", size = 10);
           echo(m.position); echo(m.size); echo(m.ascent);
           echo(m.descent); echo(m.offset); echo(m.advance);"#,
    );
    let p = vec2(&e[0]);
    close(p[0], 1.1392);
    close(p[1], -0.1408);
    let s = vec2(&e[1]);
    close(s[0], 29.929);
    close(s[1], 10.208);
    close(num(&e[2]), 10.0672);
    close(num(&e[3]), -0.1408);
    let o = vec2(&e[4]);
    close(o[0], 0.0);
    close(o[1], 0.0);
    let a = vec2(&e[5]);
    close(a[0], 31.6501);
    close(a[1], 0.0);
}

#[test]
fn textmetrics_alignment_shifts_position_not_ascent() {
    // ascent/descent are baseline-relative and do NOT move with alignment;
    // position does. offset.x is -advance/2 (center) / -advance (right);
    // offset.y aligns the ink box (center / top).
    let e = echoes(
        r#"c = textmetrics("Hello", size = 10, halign = "center", valign = "center");
           echo(c.offset); echo(c.position); echo(c.ascent); echo(c.descent);
           r = textmetrics("Hello", size = 10, halign = "right", valign = "top");
           echo(r.offset);"#,
    );
    let co = vec2(&e[0]);
    close(co[0], -15.8251);
    close(co[1], -4.9632);
    let cp = vec2(&e[1]);
    close(cp[0], -14.6859);
    close(cp[1], -5.104);
    close(num(&e[2]), 10.0672); // unchanged by alignment
    close(num(&e[3]), -0.1408);
    let ro = vec2(&e[4]);
    close(ro[0], -31.6501);
    close(ro[1], -10.0672);
}

#[test]
fn textmetrics_spacing_widens_without_moving_first_glyph() {
    // spacing scales the advances (and thus width) but not the first glyph's
    // left bearing. Oracle: advance 63.3002, size.x 53.8548, position.x 1.1392.
    let e = echoes(
        r#"m = textmetrics("Hello", size = 10, spacing = 2);
           echo(m.advance); echo(m.size); echo(m.position);"#,
    );
    close(vec2(&e[0])[0], 63.3002);
    close(vec2(&e[1])[0], 53.8548);
    close(vec2(&e[2])[0], 1.1392);
}

#[test]
fn textmetrics_empty_is_all_zero() {
    let e = echoes(
        r#"m = textmetrics("", size = 10);
           echo(m.position); echo(m.size); echo(m.advance);
           echo(m.ascent); echo(m.descent);"#,
    );
    assert_eq!(payload(&e[0]), "[0, 0]");
    assert_eq!(payload(&e[1]), "[0, 0]");
    assert_eq!(payload(&e[2]), "[0, 0]");
    assert_eq!(payload(&e[3]), "0");
    assert_eq!(payload(&e[4]), "0");
}

#[test]
fn fontmetrics_matches_oracle() {
    let e = echoes(
        r#"m = fontmetrics(size = 10);
           echo(m.nominal.ascent); echo(m.nominal.descent);
           echo(m.max.ascent); echo(m.max.descent); echo(m.interline);
           echo(m.font.family); echo(m.font.style);"#,
    );
    close(num(&e[0]), 12.5733);
    close(num(&e[1]), -2.9433);
    close(num(&e[2]), 13.6109);
    close(num(&e[3]), -4.2114);
    close(num(&e[4]), 15.9709);
    assert_eq!(payload(&e[5]), "\"Liberation Sans\"");
    assert_eq!(payload(&e[6]), "\"Regular\"");
}

#[test]
fn fontmetrics_resolves_requested_family_and_style() {
    let e = echoes(
        r#"m = fontmetrics(size = 10, font = "Liberation Serif:style=Bold");
           echo(m.font.family); echo(m.font.style); echo(m.nominal.ascent);"#,
    );
    assert_eq!(payload(&e[0]), "\"Liberation Serif\"");
    assert_eq!(payload(&e[1]), "\"Bold\"");
    close(num(&e[2]), 12.3766);
}

#[test]
fn object_field_access_forms() {
    // `.name`, `["name"]`, nesting, indexing a vector field, a missing field
    // (undef), and member access inside a function body (which the VM refuses
    // to compile, exercising the tree-walk fallback).
    let e = echoes(
        r#"m = textmetrics("Hello", size = 10);
           echo(m.size.x == m["size"][0]);
           echo(m["ascent"] == m.ascent);
           function w(s) = textmetrics(s, size = 10).size.x;
           echo(w("Hello"));
           echo(m.no_such_field);"#,
    );
    assert_eq!(payload(&e[0]), "true");
    assert_eq!(payload(&e[1]), "true");
    close(num(&e[2]), 29.929);
    assert_eq!(payload(&e[3]), "undef");
}

#[test]
fn object_echo_format() {
    // OpenSCAD prints an object as `{ key = value; … }`.
    let e = echoes(r#"echo(fontmetrics(size = 10).font);"#);
    assert_eq!(
        payload(&e[0]),
        "{ family = \"Liberation Sans\"; style = \"Regular\"; }"
    );
}
