//! CLI flag behavior, driven against the built `openrscad` binary. These prove
//! the workflow surface (exit codes, stdout/stderr shape, output plumbing) that
//! the in-process eval tests cannot: they exercise argument parsing and the
//! `main` glue itself.

use std::path::PathBuf;
use std::process::Command;

/// Path to the binary under test, provided by Cargo for integration tests.
fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_openrscad"))
}

/// A unique temp path with the given suffix (no external crate). A process-wide
/// counter plus the pid guards against collisions between parallel tests.
fn tmp(suffix: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut p = std::env::temp_dir();
    p.push(format!("orscad-test-{}-{n}-{suffix}", std::process::id()));
    p
}

/// Write a `.scad` source to a fresh temp file and return its path.
fn scad(source: &str) -> PathBuf {
    let p = tmp("in.scad");
    std::fs::write(&p, source).unwrap();
    p
}

#[test]
fn info_runs_without_an_input_file() {
    let out = bin().arg("--info").output().unwrap();
    assert!(out.status.success(), "--info should exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("OpenRSCAD version:"));
    assert!(stdout.contains("Geometry backend:"));
}

#[test]
fn enable_is_a_no_op() {
    let src = scad("cube(3);");
    let stl = tmp("out.stl");
    let out = bin()
        .arg(&src)
        .args(["--enable", "roof", "--enable", "manifold"])
        .arg("-o")
        .arg(&stl)
        .output()
        .unwrap();
    assert!(out.status.success(), "--enable must not break the run");
    assert!(stl.exists());
}

#[test]
fn quiet_suppresses_echo_and_stats() {
    let src = scad("echo(\"hello\"); cube(3);");
    let stl = tmp("out.stl");
    let out = bin()
        .arg(&src)
        .arg("-q")
        .arg("-o")
        .arg(&stl)
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stdout.contains("hello"), "echo should be suppressed by -q");
    assert!(
        !stderr.contains("rendered"),
        "stats block should be suppressed by -q"
    );
}

#[test]
fn hardwarnings_fails_on_a_warning() {
    // Reassigning a top-level variable warns ("assigned again later").
    let src = scad("a = 1; a = 2; cube(a);");
    let clean = scad("cube(3);");
    let stl = tmp("out.stl");

    let warned = bin()
        .arg(&src)
        .arg("--hardwarnings")
        .arg("-o")
        .arg(&stl)
        .output()
        .unwrap();
    assert!(
        !warned.status.success(),
        "--hardwarnings must exit non-zero when the model warns"
    );

    let ok = bin()
        .arg(&clean)
        .arg("--hardwarnings")
        .arg("-o")
        .arg(&stl)
        .output()
        .unwrap();
    assert!(
        ok.status.success(),
        "--hardwarnings must exit 0 for a clean model"
    );
}

#[test]
fn wrl_and_pdf_export_write_files() {
    let solid = scad("cube([10, 8, 6]);");
    let profile = scad("square([10, 8]);");

    let wrl = tmp("out.wrl");
    assert!(bin()
        .arg(&solid)
        .arg("-o")
        .arg(&wrl)
        .status()
        .unwrap()
        .success());
    let wrl_text = std::fs::read_to_string(&wrl).unwrap();
    assert!(wrl_text.starts_with("#VRML V2.0 utf8"));

    let pdf = tmp("out.pdf");
    assert!(bin()
        .arg(&profile)
        .arg("-o")
        .arg(&pdf)
        .status()
        .unwrap()
        .success());
    let pdf_bytes = std::fs::read(&pdf).unwrap();
    assert!(pdf_bytes.starts_with(b"%PDF-1.4"));
}

#[test]
fn invalid_output_suffix_is_rejected() {
    let src = scad("cube(3);");
    let bad = tmp("out.xyz");
    let out = bin().arg(&src).arg("-o").arg(&bad).output().unwrap();
    assert!(
        !out.status.success(),
        "an unknown suffix must be an error, not a silent STL"
    );
    assert!(!bad.exists(), "nothing should be written for a bad suffix");
}
