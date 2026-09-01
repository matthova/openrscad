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
fn define_accepts_arbitrary_expressions() {
    // A nested matrix and an arithmetic expression, neither a flat literal.
    let src = scad("m = 0; r = 0; echo(m); echo(r);");
    let out = bin()
        .arg(&src)
        .args(["-D", "m=[[1,2],[3,4]]", "-D", "r=sqrt(2)*2", "--check"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("[[1, 2], [3, 4]]"),
        "nested matrix -D failed: {stdout}"
    );
    assert!(stdout.contains("2.82843"), "expression -D failed: {stdout}");
}

#[test]
fn export_format_overrides_the_suffix() {
    let src = scad("cube(3);");
    // Ask for OFF but name the file `.stl`: the explicit format wins.
    let out = tmp("mismatch.stl");
    assert!(bin()
        .arg(&src)
        .args(["--export-format", "off"])
        .arg("-o")
        .arg(&out)
        .arg("-q")
        .status()
        .unwrap()
        .success());
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(
        text.starts_with("OFF"),
        "expected OFF content, got: {text:.10}"
    );
}

#[test]
fn export_format_asciistl_pins_ascii() {
    let src = scad("cube(3);");
    let out = tmp("ascii.stl");
    assert!(bin()
        .arg(&src)
        .args(["--export-format", "asciistl"])
        .arg("-o")
        .arg(&out)
        .arg("-q")
        .status()
        .unwrap()
        .success());
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.starts_with("solid"), "expected ASCII STL, got binary");
}

#[test]
fn export_format_rejects_unknown() {
    let src = scad("cube(3);");
    let out = tmp("x.stl");
    assert!(!bin()
        .arg(&src)
        .args(["--export-format", "nope"])
        .arg("-o")
        .arg(&out)
        .status()
        .unwrap()
        .success());
}

#[test]
fn deps_file_lists_imported_and_used_files() {
    // Build an STL to import, plus a `use`d library, then check both land in
    // the dependency rule — the important case is `import`, which flows through
    // `load_bytes`, not just `include`/`use`.
    let cube_stl = tmp("cube.stl");
    assert!(bin()
        .arg(scad("cube(3);"))
        .arg("-o")
        .arg(&cube_stl)
        .arg("-q")
        .status()
        .unwrap()
        .success());

    let src = scad(&format!(
        "import(\"{}\"); cube(1);",
        cube_stl.to_string_lossy()
    ));
    let out = tmp("out.stl");
    let deps = tmp("out.deps");
    assert!(bin()
        .arg(&src)
        .arg("-o")
        .arg(&out)
        .arg("-d")
        .arg(&deps)
        .arg("-m")
        .arg("echo rebuild")
        .arg("-q")
        .status()
        .unwrap()
        .success());

    let rule = std::fs::read_to_string(&deps).unwrap();
    assert!(rule.starts_with(&out.to_string_lossy().replace(' ', "\\ ")));
    assert!(
        rule.contains("cube.stl"),
        "imported STL must appear in deps: {rule}"
    );
    assert!(rule.contains("\techo rebuild"), "-m recipe missing: {rule}");
}

#[test]
fn check_parameter_ranges_catches_out_of_range() {
    let model = scad("width = 5; // [0:10]\ncube(width);");
    let set = tmp("set.json");
    std::fs::write(
        &set,
        r#"{"parameterSets":{"big":{"width":"20"}},"fileFormatVersion":"1"}"#,
    )
    .unwrap();

    // Out of range → non-zero exit.
    let bad = bin()
        .arg(&model)
        .arg("-p")
        .arg(&set)
        .args(["-P", "big", "--check-parameter-ranges", "--check"])
        .output()
        .unwrap();
    assert!(
        !bad.status.success(),
        "out-of-range slider value should fail validation"
    );
    assert!(String::from_utf8_lossy(&bad.stderr).contains("slider range"));
}

#[test]
fn params_file_without_set_is_tolerated() {
    // `-p` without `-P` selects no set and must not error (OpenSCAD behavior).
    let model = scad("width = 5; cube(width);");
    let set = tmp("set.json");
    std::fs::write(
        &set,
        r#"{"parameterSets":{"s":{"width":"7"}},"fileFormatVersion":"1"}"#,
    )
    .unwrap();
    let out = bin()
        .arg(&model)
        .arg("-p")
        .arg(&set)
        .arg("--check")
        .output()
        .unwrap();
    assert!(out.status.success(), "-p without -P must be tolerated");
}

#[test]
fn script_viewport_drives_the_camera() {
    // A script that sets its own `$vp*` must render from that camera. Proven by
    // equality with the equivalent explicit `--camera` gimbal (matching fov).
    let model = "\
        $vpt = [0,0,0];\n\
        $vpr = [60,0,30];\n\
        $vpd = 120;\n\
        $vpf = 45;\n\
        cube(10, center=true);\n";
    let src = scad(model);

    let from_script = tmp("vp_script.png");
    assert!(bin()
        .arg(&src)
        .arg("-o")
        .arg(&from_script)
        .arg("-q")
        .status()
        .unwrap()
        .success());

    let from_camera = tmp("vp_cam.png");
    assert!(bin()
        .arg(&src)
        .arg("-o")
        .arg(&from_camera)
        .args(["--camera", "0,0,0,60,0,30,120", "-q"])
        .status()
        .unwrap()
        .success());

    let a = std::fs::read(&from_script).unwrap();
    let b = std::fs::read(&from_camera).unwrap();
    assert_eq!(
        a, b,
        "a script-set $vp* camera must match the equivalent --camera"
    );

    // A model with no $vp* auto-frames instead, so it must NOT match the fixed
    // 120-unit gimbal above.
    let plain = scad("cube(10, center=true);");
    let auto = tmp("auto.png");
    assert!(bin()
        .arg(&plain)
        .arg("-o")
        .arg(&auto)
        .arg("-q")
        .status()
        .unwrap()
        .success());
    assert_ne!(
        std::fs::read(&auto).unwrap(),
        a,
        "a model without $vp* should auto-frame, not use the script camera"
    );
}

#[test]
fn colorscheme_rejects_unknown_and_accepts_known() {
    let src = scad("cube(3);");
    let png = tmp("cs.png");
    assert!(bin()
        .arg(&src)
        .arg("-o")
        .arg(&png)
        .args(["--colorscheme", "Tomorrow", "-q"])
        .status()
        .unwrap()
        .success());
    assert!(!bin()
        .arg(&src)
        .arg("-o")
        .arg(&png)
        .args(["--colorscheme", "Nope", "-q"])
        .status()
        .unwrap()
        .success());
}

#[test]
fn animate_sharding_renders_a_subset() {
    let src = scad("cube($t * 10 + 1);");
    let base = tmp("shard.png");
    assert!(bin()
        .arg(&src)
        .arg("-o")
        .arg(&base)
        .args(["--animate", "4", "--animate_sharding", "0/2", "-q"])
        .status()
        .unwrap()
        .success());
    // Frames 0 and 2 of 0..4 belong to shard 0/2; 1 and 3 do not.
    let dir = base.parent().unwrap();
    let stem = base.file_stem().unwrap().to_string_lossy();
    assert!(dir.join(format!("{stem}00000.png")).exists());
    assert!(dir.join(format!("{stem}00002.png")).exists());
    assert!(!dir.join(format!("{stem}00001.png")).exists());
    assert!(!dir.join(format!("{stem}00003.png")).exists());
}

#[test]
fn summary_text_and_json() {
    let src = scad("cube([10, 8, 6]);");
    let stl = tmp("out.stl");

    // Selected sections print to stdout.
    let out = bin()
        .arg(&src)
        .arg("-o")
        .arg(&stl)
        .args(["--summary", "volume,geometry"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Volume: 480"), "summary text: {stdout}");
    assert!(stdout.contains("Facets: 12"), "summary text: {stdout}");

    // `--summary-file` writes JSON with the requested fields.
    let json = tmp("sum.json");
    assert!(bin()
        .arg(&src)
        .arg("-o")
        .arg(&stl)
        .arg("--summary-file")
        .arg(&json)
        .arg("-q")
        .status()
        .unwrap()
        .success());
    let text = std::fs::read_to_string(&json).unwrap();
    assert!(text.contains("\"volume\":480"), "summary json: {text}");
    assert!(text.contains("\"facets\":12"), "summary json: {text}");

    // An unknown section is rejected.
    assert!(!bin()
        .arg(&src)
        .arg("-o")
        .arg(&stl)
        .args(["--summary", "nope"])
        .status()
        .unwrap()
        .success());
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
