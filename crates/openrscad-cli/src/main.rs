//! `openrscad` — command-line renderer for the OpenRSCAD OpenSCAD reimplementation.

use anyhow::{Context, Result};
use clap::Parser;
use openrscad_eval::{FileResolver, LoadedFile};
use std::path::{Path, PathBuf};
use std::time::Instant;

mod raster;

/// Resolves `include`/`use` paths from disk: relative to the including file,
/// then each `OPENSCADPATH` library directory.
struct DiskResolver {
    libs: Vec<PathBuf>,
}

impl FileResolver for DiskResolver {
    fn load(&self, path: &str, from_dir: &str) -> Option<LoadedFile> {
        let candidates = std::iter::once(Path::new(from_dir).join(path))
            .chain(self.libs.iter().map(|l| l.join(path)));
        for c in candidates {
            if let Ok(source) = std::fs::read_to_string(&c) {
                let key = std::fs::canonicalize(&c)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| c.to_string_lossy().into_owned());
                let dir = c
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                return Some(LoadedFile { key, source, dir });
            }
        }
        None
    }

    fn load_bytes(&self, path: &str, from_dir: &str) -> Option<Vec<u8>> {
        let candidates = std::iter::once(Path::new(from_dir).join(path))
            .chain(self.libs.iter().map(|l| l.join(path)));
        candidates.into_iter().find_map(|c| std::fs::read(&c).ok())
    }
}

/// A fast OpenSCAD-compatible renderer (M0 subset).
#[derive(Parser, Debug)]
#[command(name = "openrscad", version, about)]
struct Cli {
    /// Input `.scad` file. Optional only with `--info`.
    input: Option<PathBuf>,

    /// Output file. Format by extension: 3D `.stl`/`.off`/`.obj`/`.3mf`/`.amf`/`.wrl`,
    /// 2D `.dxf`/`.svg`/`.pdf`, tree `.csg`. If omitted, only prints model statistics.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// STL output format.
    #[arg(long, value_enum, default_value_t = StlFormat::Binary)]
    format: StlFormat,

    /// Explicit export format, overriding the `-o` suffix. Accepts OpenSCAD
    /// spellings: `stl`/`binstl`/`asciistl`, `off`, `obj`, `3mf`, `amf`, `dxf`,
    /// `svg`, `pdf`, `wrl`, `csg`, `png`, and `echo` (equivalent to `--check`).
    #[arg(long, value_name = "FMT")]
    export_format: Option<String>,

    /// Print echo/warning output only; do not render geometry.
    #[arg(long)]
    check: bool,

    /// Enable an experimental feature (repeatable). Accepted for compatibility
    /// with upstream scripts, but ignored: all experimental features are always
    /// on in OpenRSCAD.
    #[arg(long, value_name = "FEATURE")]
    enable: Vec<String>,

    /// OpenCSG preview polygon limit. Accepted and ignored (OpenRSCAD has no
    /// OpenCSG preview); rendering is unaffected.
    #[arg(long, value_name = "N")]
    csglimit: Option<u64>,

    /// Suppress echoes, warnings, and the render statistics; errors still go to
    /// stderr and still set the exit code.
    #[arg(short = 'q', long)]
    quiet: bool,

    /// Exit non-zero if the model produced any warnings (after printing them).
    #[arg(long)]
    hardwarnings: bool,

    /// Print version, kernel backend, enabled features, and library paths, then
    /// exit.
    #[arg(long)]
    info: bool,

    /// Override a top-level parameter, e.g. `-D width=20` or `-D label="hi"`.
    /// Repeatable. Values are literals (number/bool/string/vector), matching
    /// the customizer.
    #[arg(short = 'D', long = "param", value_name = "NAME=VALUE")]
    params: Vec<String>,

    /// Customizer parameter-set file (OpenSCAD `.json`). Use with `-P` to select
    /// a named set; `-D` overrides still win.
    #[arg(short = 'p', long = "params-file", value_name = "FILE")]
    params_file: Option<PathBuf>,

    /// Name of the parameter set to apply from `-p`'s file.
    #[arg(short = 'P', long = "set", value_name = "NAME")]
    param_set: Option<String>,

    /// Render an animation: `N` frames with `$t` sweeping 0→1, written as
    /// `out00000.png`… (requires a `.png` `-o`).
    #[arg(long, value_name = "N")]
    animate: Option<u32>,

    /// PNG image size, e.g. `--imgsize 800,600` (or `800x600`). Default 512,512.
    #[arg(long, value_name = "W,H")]
    imgsize: Option<String>,

    /// PNG camera: eye/center `ex,ey,ez,cx,cy,cz` (6 values) or OpenSCAD gimbal
    /// `tx,ty,tz,rx,ry,rz,dist` (7). Omit to auto-frame the model.
    #[arg(long, value_name = "…")]
    camera: Option<String>,

    /// PNG projection.
    #[arg(long, value_enum, default_value_t = Proj::Perspective)]
    projection: Proj,

    /// PNG: frame the whole model, ignoring the camera distance.
    #[arg(long)]
    viewall: bool,

    /// PNG: shift the model so its center is the view target.
    #[arg(long)]
    autocenter: bool,

    /// Force `$preview = false` (F6 semantics). Geometry export already implies
    /// this; use it to render a PNG the way an exact export would.
    #[arg(long, conflicts_with = "preview")]
    render: bool,

    /// Force `$preview = true` (F5 semantics), even for a geometry export.
    #[arg(long)]
    preview: bool,
}

impl Cli {
    /// The `$preview` value this invocation gives the script, mirroring
    /// OpenSCAD 2024.12: false whenever an exact render happens (mesh or 2D
    /// vector export, and our stats output, which reports exact volume/area),
    /// true for echo-only runs and PNG preview rasters.
    fn render_mode(&self) -> openrscad_eval::RenderMode {
        use openrscad_eval::RenderMode;
        if self.preview {
            return RenderMode::Preview;
        }
        if self.render {
            return RenderMode::Exact;
        }
        // `--check` never renders geometry, matching `--export-format=echo`.
        if self.check {
            return RenderMode::Preview;
        }
        // An explicit `--export-format` decides the mode when present: `echo`
        // is echo-only (preview), PNG/CSG skip the exact render, the rest render
        // exactly. An unrecognized spelling is reported later, by `run`.
        if let Some(fmt) = &self.export_format {
            return match OutputFormat::from_export_format(fmt) {
                Ok(None) => RenderMode::Preview, // echo
                Ok(Some((f, _))) if f.skips_exact_render() => RenderMode::Preview,
                _ => RenderMode::Exact,
            };
        }
        // PNG (including `--animate`) and `.csg` are produced without an exact
        // render upstream. An unclassifiable suffix is reported later, by `run`.
        let no_render = self.animate.is_some()
            || self.output.as_deref().is_some_and(|p| {
                OutputFormat::from_path(p).is_ok_and(OutputFormat::skips_exact_render)
            });
        if no_render {
            RenderMode::Preview
        } else {
            RenderMode::Exact
        }
    }
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum StlFormat {
    Binary,
    Ascii,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum Proj {
    Perspective,
    Ortho,
}

/// The export format an `-o` path selects, by suffix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputFormat {
    Csg,
    Stl,
    Off,
    Obj,
    ThreeMf,
    Amf,
    Dxf,
    Svg,
    Pdf,
    Wrl,
    Png,
}

impl OutputFormat {
    /// Classify an output path by its suffix, case-insensitively as OpenSCAD
    /// does (`out.STL` is an STL).
    ///
    /// An unrecognized suffix is an error, not a silent fallback: writing STL
    /// bytes to `model.csg` and exiting 0 tells the user they got a CSG tree
    /// when they did not. OpenSCAD rejects the same input with
    /// "Invalid suffix foo" and writes nothing.
    fn from_path(path: &Path) -> Result<Self> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        Ok(match ext.as_str() {
            "stl" => OutputFormat::Stl,
            "off" => OutputFormat::Off,
            "obj" => OutputFormat::Obj,
            "3mf" => OutputFormat::ThreeMf,
            "amf" => OutputFormat::Amf,
            "dxf" => OutputFormat::Dxf,
            "svg" => OutputFormat::Svg,
            "pdf" => OutputFormat::Pdf,
            "wrl" => OutputFormat::Wrl,
            "png" => OutputFormat::Png,
            "csg" => OutputFormat::Csg,
            "" => anyhow::bail!(
                "output path '{}' has no suffix; \
                 expected one of: stl, off, obj, 3mf, amf, dxf, svg, pdf, wrl, png, csg",
                path.display()
            ),
            other => anyhow::bail!(
                "invalid output suffix '{other}'; \
                 expected one of: stl, off, obj, 3mf, amf, dxf, svg, pdf, wrl, png, csg"
            ),
        })
    }

    /// Resolve an explicit `--export-format` spelling to a format, plus an
    /// optional STL sub-format when the name pins ASCII/binary. `echo` returns
    /// `Ok(None)` — the caller treats it like `--check`.
    fn from_export_format(name: &str) -> Result<Option<(OutputFormat, Option<StlFormat>)>> {
        let n = name.trim().to_ascii_lowercase();
        Ok(Some(match n.as_str() {
            "echo" => return Ok(None),
            "stl" => (OutputFormat::Stl, None),
            "binstl" => (OutputFormat::Stl, Some(StlFormat::Binary)),
            "asciistl" => (OutputFormat::Stl, Some(StlFormat::Ascii)),
            "off" => (OutputFormat::Off, None),
            "obj" => (OutputFormat::Obj, None),
            "3mf" => (OutputFormat::ThreeMf, None),
            "amf" => (OutputFormat::Amf, None),
            "dxf" => (OutputFormat::Dxf, None),
            "svg" => (OutputFormat::Svg, None),
            "pdf" => (OutputFormat::Pdf, None),
            "wrl" | "vrml" => (OutputFormat::Wrl, None),
            "csg" => (OutputFormat::Csg, None),
            "png" => (OutputFormat::Png, None),
            other => anyhow::bail!(
                "unsupported --export-format '{other}'; expected one of: \
                 stl, binstl, asciistl, off, obj, 3mf, amf, dxf, svg, pdf, wrl, csg, png, echo"
            ),
        }))
    }

    /// Whether this format is produced *without* an exact render, and so keeps
    /// `$preview` true (see [`Cli::render_mode`]). PNG is a preview raster and
    /// `.csg` serializes the tree without rendering at all; both report true
    /// upstream.
    fn skips_exact_render(self) -> bool {
        matches!(self, OutputFormat::Png | OutputFormat::Csg)
    }
}

/// Parse `--imgsize` (`W,H` or `WxH`).
fn parse_imgsize(s: &str) -> Result<(u32, u32)> {
    let parts: Vec<&str> = s.split([',', 'x', 'X']).collect();
    if parts.len() != 2 {
        anyhow::bail!("--imgsize expects W,H (e.g. 800,600)");
    }
    let w = parts[0].trim().parse().context("--imgsize width")?;
    let h = parts[1].trim().parse().context("--imgsize height")?;
    Ok((w, h))
}

/// Parse `--camera`: 6 numbers → eye/center, 7 → OpenSCAD gimbal.
fn parse_camera(s: &str) -> Result<raster::Camera> {
    let nums: Vec<f64> = s
        .split(',')
        .map(|p| p.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .context("--camera expects comma-separated numbers")?;
    match nums.len() {
        6 => Ok(raster::Camera::Eye {
            eye: [nums[0], nums[1], nums[2]],
            center: [nums[3], nums[4], nums[5]],
        }),
        7 => Ok(raster::Camera::Gimbal {
            target: [nums[0], nums[1], nums[2]],
            rot: [nums[3], nums[4], nums[5]],
            dist: nums[6],
        }),
        n => anyhow::bail!("--camera expects 6 (eye,center) or 7 (gimbal) numbers, got {n}"),
    }
}

/// Build the PNG render options from the CLI flags (`--imgsize`/`--camera`/…).
fn build_render_opts(cli: &Cli) -> Result<raster::RenderOpts> {
    let (width, height) = match &cli.imgsize {
        Some(s) => parse_imgsize(s)?,
        None => (512, 512),
    };
    let camera = match &cli.camera {
        Some(s) => parse_camera(s)?,
        None => raster::Camera::Auto,
    };
    let projection = match cli.projection {
        Proj::Perspective => raster::Projection::Perspective { fov_deg: 45.0 },
        Proj::Ortho => raster::Projection::Ortho,
    };
    Ok(raster::RenderOpts {
        width,
        height,
        camera,
        projection,
        viewall: cli.viewall,
        autocenter: cli.autocenter,
        ..Default::default()
    })
}

/// Rasterize one CSG tree to PNG bytes, colored via the B3 groups (`%` dropped).
fn render_frame_png(node: &openrscad_ir::Node, opts: &raster::RenderOpts) -> Result<Vec<u8>> {
    let groups = openrscad_geom::render_groups(node).context("rendering color groups")?;
    let colored: Vec<(&openrscad_geom::Mesh, [f32; 4])> = groups
        .iter()
        .filter(|g| g.mode != openrscad_geom::DisplayMode::Background)
        .map(|g| (&g.mesh, g.color))
        .collect();
    raster::render_png(&colored, opts).map_err(|e| anyhow::anyhow!("png render: {e}"))
}

/// `--animate N`: render `N` frames with `$t` swept 0→1, written as numbered PNGs.
fn run_animation(
    program: &openrscad_syntax::Program,
    resolver: &DiskResolver,
    base_dir: &str,
    base_overrides: &[(String, openrscad_eval::Value)],
    cli: &Cli,
    n: u32,
) -> Result<()> {
    let path = cli
        .output
        .as_ref()
        .context("--animate requires -o out.png")?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext != "png" {
        anyhow::bail!("--animate requires a .png output file");
    }
    if n == 0 {
        anyhow::bail!("--animate N must be >= 1");
    }
    let opts = build_render_opts(cli)?;
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("frame");
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    // Zero-pad to at least 5 digits (matching OpenSCAD), more if needed.
    let pad = 5.max((n - 1).to_string().len());
    for i in 0..n {
        let t = i as f64 / n as f64;
        let mut ov = base_overrides.to_vec();
        ov.push((
            "$t".to_string(),
            openrscad_eval::value_from_param(&openrscad_syntax::customizer::ParamValue::Number(t)),
        ));
        let out = openrscad_eval::eval_program_with_mode(
            program,
            resolver,
            base_dir,
            &ov,
            cli.render_mode(),
        )
        .map_err(|e| anyhow::anyhow!("frame {i}: {}", e.message))?;
        let bytes = render_frame_png(&out.node, &opts)?;
        let fname = format!("{stem}{i:0pad$}.png");
        std::fs::write(dir.join(&fname), bytes)?;
    }
    eprintln!("wrote {n} frames to {}", dir.display());
    Ok(())
}

/// The experimental features that are always on (see `--enable`), reported by
/// `--info` so no false parity claim is made about them being toggleable.
const ALWAYS_ON_FEATURES: &[&str] = &["roof", "object-values", "fill", "lazy-union"];

/// `--info`: print version, kernel backend, always-on features, and library
/// paths, mirroring OpenSCAD's `--info`.
fn print_info(_cli: &Cli) {
    println!("OpenRSCAD version: {}", env!("CARGO_PKG_VERSION"));
    println!("Geometry backend: Manifold (native)");
    println!(
        "Always-on experimental features: {}",
        ALWAYS_ON_FEATURES.join(", ")
    );
    let osp = std::env::var("OPENSCADPATH").unwrap_or_default();
    println!(
        "OPENSCADPATH: {}",
        if osp.is_empty() { "(unset)" } else { &osp }
    );
}

fn main() -> Result<()> {
    // Run on a worker thread with a large stack: recursive libraries (e.g.
    // BOSL2's attachment system) can nest the evaluator deeply, and OpenSCAD
    // itself runs with a large stack for the same reason.
    std::thread::Builder::new()
        .stack_size(256 << 20) // 256 MiB
        .spawn(run)
        .context("spawning worker thread")?
        .join()
        .map_err(|_| anyhow::anyhow!("worker thread panicked"))?
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    // `--info` prints environment facts and exits; it needs no input file.
    if cli.info {
        print_info(&cli);
        return Ok(());
    }

    let input = cli
        .input
        .as_ref()
        .context("no input file given (only `--info` may be run without one)")?;

    // Resolve the export format. An explicit `--export-format` overrides the
    // `-o` suffix (and may pin binary/ASCII STL, or request echo-only). Reject
    // an unusable choice before doing any work, so a long render is not thrown
    // away on a typo.
    let mut stl_format = cli.format;
    let mut echo_only = cli.check;
    let format = if let Some(fmt) = &cli.export_format {
        match OutputFormat::from_export_format(fmt)? {
            None => {
                echo_only = true;
                None
            }
            Some((f, stl)) => {
                if let Some(stl) = stl {
                    stl_format = stl;
                }
                Some(f)
            }
        }
    } else {
        cli.output
            .as_deref()
            .map(OutputFormat::from_path)
            .transpose()?
    };

    let src =
        std::fs::read_to_string(input).with_context(|| format!("reading {}", input.display()))?;

    // Make the OS's installed fonts available to `text(font="…")` (matching
    // OpenSCAD's fontconfig behavior). Only pay the font-dir scan when the model
    // might actually use `text()`; the bundled Liberation family is always there.
    if src.contains("text") {
        openrscad_eval::register_system_fonts();
    }

    // Parse.
    let program = openrscad_syntax::parse(&src)
        .map_err(|e| anyhow::anyhow!("parse error at {:?}: {}", e.span, e.message))?;

    // Evaluate, resolving include/use relative to the input file + OPENSCADPATH.
    let base_dir = input
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| ".".to_string());
    let libs = std::env::var("OPENSCADPATH")
        .unwrap_or_default()
        .split(':')
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect();
    let resolver = DiskResolver { libs };

    // Parameter overrides: a customizer parameter set (`-p file.json -P set`)
    // first, then `-D name=value` on top (so `-D` wins).
    let mut overrides = Vec::new();
    if let Some(file) = &cli.params_file {
        let set = cli
            .param_set
            .as_deref()
            .context("-p requires -P <set-name> to select a parameter set")?;
        let json = std::fs::read_to_string(file)
            .with_context(|| format!("reading parameter-set file {}", file.display()))?;
        let schema = openrscad_syntax::customizer::extract(&src);
        let set_overrides =
            openrscad_syntax::customizer::parameter_set_overrides(&json, set, &schema)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
        for (name, pv) in set_overrides {
            overrides.push((name, openrscad_eval::value_from_param(&pv)));
        }
    } else if cli.param_set.is_some() {
        anyhow::bail!("-P <set-name> requires -p <file.json>");
    }
    for p in &cli.params {
        let (name, val) = p
            .split_once('=')
            .with_context(|| format!("--param must be NAME=VALUE, got '{p}'"))?;
        // OpenSCAD's `-D` takes any expression, so evaluate it against the base
        // scope (`PI`, `sqrt()`, nested vectors, …). Fall back to the flat
        // customizer literal parser only if the expression evaluator rejects it,
        // so an unquoted bareword like `-D label=hi` still works as a string the
        // way the customizer allowed.
        let value = match openrscad_eval::eval_const_expr(val.trim()) {
            Ok(v) => v,
            Err(_) => {
                let pv = openrscad_syntax::customizer::parse_value(val.trim())
                    .with_context(|| format!("invalid parameter value: '{val}'"))?;
                openrscad_eval::value_from_param(&pv)
            }
        };
        overrides.push((name.trim().to_string(), value));
    }

    // Animation: re-eval per frame with a swept `$t` and write numbered PNGs.
    if let Some(n) = cli.animate {
        return run_animation(&program, &resolver, &base_dir, &overrides, &cli, n);
    }

    let out = openrscad_eval::eval_program_with_mode(
        &program,
        &resolver,
        &base_dir,
        &overrides,
        cli.render_mode(),
    )
    .map_err(|e| anyhow::anyhow!("evaluation error: {}", e.message))?;

    if !cli.quiet {
        for line in &out.echoes {
            println!("{line}");
        }
        for w in &out.warnings {
            eprintln!("WARNING: {}", w.message);
        }
    }
    // `--hardwarnings` treats any script warning as a failure. Applies whether
    // or not `-q` silenced the messages.
    if cli.hardwarnings && !out.warnings.is_empty() {
        anyhow::bail!("{} warning(s) with --hardwarnings", out.warnings.len());
    }

    if echo_only {
        return Ok(());
    }

    // 2D vector export (DXF/SVG): write contours directly, no 3D mesh needed.
    // `.csg` is a serialization of the evaluated tree, so it needs no render at
    // all — emit it before the (potentially slow) geometry pass.
    if let (Some(path), Some(OutputFormat::Csg)) = (&cli.output, format) {
        std::fs::write(path, openrscad_eval::export_csg(&out.node))?;
        eprintln!("wrote {}", path.display());
        return Ok(());
    }

    if let (
        Some(path),
        Some(format @ (OutputFormat::Dxf | OutputFormat::Svg | OutputFormat::Pdf)),
    ) = (&cli.output, format)
    {
        let kernel = openrscad_geom::ManifoldKernel::new();
        match openrscad_geom::render_contours_with(&out.node, &kernel) {
            Ok(Some(contours)) => {
                let bytes: Vec<u8> = match format {
                    OutputFormat::Dxf => openrscad_geom::export_dxf(&contours).into_bytes(),
                    OutputFormat::Svg => openrscad_geom::export_svg(&contours).into_bytes(),
                    _ => openrscad_geom::export_pdf(&contours),
                };
                std::fs::write(path, bytes)?;
                eprintln!("wrote {} ({} contours)", path.display(), contours.len());
            }
            Ok(None) => anyhow::bail!(
                "{} export requires a 2D object",
                match format {
                    OutputFormat::Dxf => "DXF",
                    OutputFormat::Svg => "SVG",
                    _ => "PDF",
                }
            ),
            Err(e) => anyhow::bail!("rendering 2D geometry: {e}"),
        }
        return Ok(());
    }

    // Render.
    let t0 = Instant::now();
    let mut geom_cache = openrscad_geom::GeomCache::new();
    let (mesh, geom_warnings) = openrscad_geom::render_cached_warns(
        &out.node,
        &openrscad_geom::ManifoldKernel::new(),
        &mut geom_cache,
    )
    .context("rendering geometry")?;
    if !cli.quiet {
        for w in &geom_warnings {
            eprintln!("WARNING: {w}");
        }
    }
    if cli.hardwarnings && !geom_warnings.is_empty() {
        anyhow::bail!(
            "{} geometry warning(s) with --hardwarnings",
            geom_warnings.len()
        );
    }
    let elapsed = t0.elapsed();

    let manifold_ok = mesh.signed_volume() > 0.0 || mesh.is_empty();
    if !cli.quiet {
        eprintln!(
            "rendered {} triangles, {} vertices in {:.1?} (volume {:.4}, area {:.4}{})",
            mesh.tris.len(),
            mesh.verts.len(),
            elapsed,
            mesh.volume(),
            mesh.surface_area(),
            if manifold_ok {
                ""
            } else {
                ", WARNING: inward-facing"
            },
        );
    }

    if let Some(path) = &cli.output {
        let name = input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("openrscad");
        // Classified up front, so the only formats left here are the 3D ones
        // plus PNG; DXF/SVG returned above.
        let format = format.unwrap_or(OutputFormat::Stl);
        match format {
            OutputFormat::Off => std::fs::write(path, mesh.to_off())?,
            OutputFormat::Obj => std::fs::write(path, mesh.to_obj())?,
            // 3MF carries per-object color: partition into color groups (dropping
            // `%` background) and write one object per color. Falls back to the
            // fused single-object 3MF when the model uses no color.
            OutputFormat::ThreeMf if openrscad_geom::has_display_attrs(&out.node) => {
                let groups =
                    openrscad_geom::render_groups(&out.node).context("rendering color groups")?;
                let colored: Vec<(&openrscad_geom::Mesh, [f32; 4])> = groups
                    .iter()
                    .filter(|g| g.mode != openrscad_geom::DisplayMode::Background)
                    .map(|g| (&g.mesh, g.color))
                    .collect();
                std::fs::write(path, openrscad_geom::Mesh::to_3mf_colored(&colored))?
            }
            OutputFormat::ThreeMf => std::fs::write(path, mesh.to_3mf())?,
            OutputFormat::Amf => std::fs::write(path, mesh.to_amf())?,
            // VRML 2.0 carries per-Shape color: partition into color groups
            // (dropping `%` background) like 3MF, one Shape per color. Falls
            // back to a single uncolored Shape when the model uses no color.
            OutputFormat::Wrl if openrscad_geom::has_display_attrs(&out.node) => {
                let groups =
                    openrscad_geom::render_groups(&out.node).context("rendering color groups")?;
                let colored: Vec<(&openrscad_geom::Mesh, [f32; 4])> = groups
                    .iter()
                    .filter(|g| g.mode != openrscad_geom::DisplayMode::Background)
                    .map(|g| (&g.mesh, g.color))
                    .collect();
                std::fs::write(path, openrscad_geom::Mesh::to_wrl_colored(&colored))?
            }
            OutputFormat::Wrl => std::fs::write(path, mesh.to_wrl())?,
            // PNG: headless software rasterizer over the colored groups (dropping
            // `%` background), honoring --imgsize/--camera/--projection.
            OutputFormat::Png => {
                let opts = build_render_opts(&cli)?;
                std::fs::write(path, render_frame_png(&out.node, &opts)?)?;
            }
            OutputFormat::Stl if matches!(stl_format, StlFormat::Ascii) => {
                std::fs::write(path, mesh.to_ascii_stl(name))?
            }
            OutputFormat::Stl => std::fs::write(path, mesh.to_binary_stl())?,
            // Returned above; listed so a new format cannot silently fall
            // through to STL.
            OutputFormat::Dxf | OutputFormat::Svg | OutputFormat::Pdf | OutputFormat::Csg => {
                unreachable!("2D and .csg formats exported earlier")
            }
        }
        eprintln!("wrote {}", path.display());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use openrscad_eval::RenderMode;

    fn mode(args: &[&str]) -> RenderMode {
        let mut argv = vec!["openrscad"];
        argv.extend_from_slice(args);
        argv.push("model.scad");
        Cli::parse_from(argv).render_mode()
    }

    /// `$preview` per output, measured against OpenSCAD 2024.12.17: false
    /// whenever an exact render happens, true otherwise. Getting this backwards
    /// exports a different model than the user asked for, silently.
    #[test]
    fn preview_mode_follows_the_output_format() {
        // Exact renders.
        assert_eq!(mode(&["-o", "out.stl"]), RenderMode::Exact);
        assert_eq!(mode(&["-o", "out.off"]), RenderMode::Exact);
        assert_eq!(mode(&["-o", "out.3mf"]), RenderMode::Exact);
        assert_eq!(mode(&["-o", "out.dxf"]), RenderMode::Exact);
        assert_eq!(mode(&["-o", "out.svg"]), RenderMode::Exact);
        // No output still renders exactly to report volume/area.
        assert_eq!(mode(&[]), RenderMode::Exact);

        // Preview-side runs.
        assert_eq!(mode(&["--check"]), RenderMode::Preview);
        assert_eq!(mode(&["-o", "out.png"]), RenderMode::Preview);
        assert_eq!(mode(&["-o", "OUT.PNG"]), RenderMode::Preview);
        assert_eq!(
            mode(&["--animate", "4", "-o", "out.png"]),
            RenderMode::Preview
        );
    }

    #[test]
    fn render_and_preview_flags_override_the_format() {
        assert_eq!(mode(&["--render", "-o", "out.png"]), RenderMode::Exact);
        assert_eq!(mode(&["--preview", "-o", "out.stl"]), RenderMode::Preview);
        assert_eq!(mode(&["--preview", "--check"]), RenderMode::Preview);
        // `--check` is echo-only, but an explicit `--render` still wins.
        assert_eq!(mode(&["--render", "--check"]), RenderMode::Exact);
    }

    /// An unrecognized suffix is rejected rather than silently written as STL.
    /// Measured against OpenSCAD 2024.12.17: it exits non-zero and writes no
    /// file for `foo` and for a path with no suffix at all.
    #[test]
    fn output_suffixes_are_classified_case_insensitively() {
        use OutputFormat::*;
        for (name, want) in [
            ("m.stl", Stl),
            ("m.STL", Stl),
            ("m.Off", Off),
            ("m.obj", Obj),
            ("m.3MF", ThreeMf),
            ("m.amf", Amf),
            ("m.dxf", Dxf),
            ("m.SVG", Svg),
            ("m.pdf", Pdf),
            ("m.WRL", Wrl),
            ("m.png", Png),
            ("m.csg", Csg),
            ("a.b.stl", Stl),
        ] {
            assert_eq!(
                OutputFormat::from_path(Path::new(name)).unwrap(),
                want,
                "{name}"
            );
        }
    }

    #[test]
    fn unusable_output_suffixes_are_rejected() {
        for name in ["m.foo", "m", "m.", "m.txt", "m.scad"] {
            assert!(
                OutputFormat::from_path(Path::new(name)).is_err(),
                "{name} should be rejected, not silently written as STL"
            );
        }
    }

    #[test]
    fn csg_export_keeps_preview_true() {
        // `.csg` serializes the tree without rendering, so upstream reports
        // $preview true for it, as it does for a PNG.
        assert_eq!(mode(&["-o", "out.csg"]), RenderMode::Preview);
        assert_eq!(mode(&["-o", "out.stl"]), RenderMode::Exact);
    }

    #[test]
    fn render_and_preview_are_mutually_exclusive() {
        assert!(Cli::try_parse_from(["openrscad", "--render", "--preview", "m.scad"]).is_err());
    }
}
