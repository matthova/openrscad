# Track G (M11) — OpenSCAD feature parity beyond the 2021.01 language core

**Status:** active. Planned 2026-08-31 at `2bb1a27`, against the public
OpenSCAD manual, the Wikibook, and black-box observation of the `openscad`
binary. Clean-room: no OpenSCAD source is used.


## Context

The question "what's left to reach feature parity with OpenSCAD?" has a
surprising answer: **the language is already there.**
`compatibility/openscad-2021.01.json` inventories 194 features with
**zero `missing`** — 175 oracle-verified, 14 implemented-but-unoracled, 3
permanent and 2 warned divergences. `xtask echo` is 35/35, `xtask geom` 130/130,
`xtask bosl2` 511/513. Track F closed the last P0 silent-geometry gaps.

So parity now means the surfaces Track F deliberately scoped **out**:

> "CLI workflow parity is tracked separately from `.scad` program semantics…"
> "Experimental features…, GUI pixel parity… are not part of the core exit
> criterion." — `docs/roadmap/track-f-measured-openscad-compatibility.md:35-41`

Those exclusions were right for M10 — they kept "compatible" from meaning "the
buttons look the same." But they are exactly where a user switching from
OpenSCAD still hits a wall: `openrscad` can't be dropped into a Makefile, can't
write WRL/PDF, doesn't know `--enable`, has no `fill()`, silently emits nothing
for a concave `roof()`, and the viewport has no color schemes or standard view
presets.

Track G inventories and closes those. Outcome: `openrscad` is a drop-in for
`openscad` in scripts and CI, and the app covers the OpenSCAD GUI's feature set.

**Decisions already made** (from clarification):
- Experimental features (`roof`, object values, `fill`, lazy-union) stay
  **always-on** as `roof()` is today. `--enable=<x>` is accepted and ignored so
  upstream scripts don't break; each feature gets an experimental manifest label
  so we make no false parity claim.
- GUI parity is its own workstream, not folded into CLI work.
- Deliverable: the roadmap doc **plus** the first batch implemented.

## Constraints (inherited, non-negotiable)

- **Clean-room** (`CONTRIBUTING.md`): specs come from the public manual, the
  Wikibook, and black-box observation of the `openscad` binary. Never read
  OpenSCAD source.
- **`openscad` is NOT installed in this workspace** (`which openscad` → not
  found). No new golden can be blessed here. Batch 1 is chosen so it needs
  none: CLI behavior is proven with Rust integration tests against our own
  binary. Anything needing the oracle is marked `implemented`, not `verified`,
  and left for a bless pass on the oracle machine.
- Dual kernels; tree-walk is reference semantics; every user-facing change ships
  a changeset in the same PR (`CLAUDE.md`).

---

## Workstreams

### G1 — CLI & workflow parity  ← **batch 1, implement now**

`crates/openrscad-cli/src/main.rs` has 16 flags; OpenSCAD has ~35. Missing
entirely: `--enable`, `--hardwarnings`, `-q/--quiet`, `-d/--deps_file`,
`-m/--make`, `--export-format`, `--summary`/`--summary-file`, `--csglimit`,
`--check-parameters`/`--check-parameter-ranges`, `--colorscheme`, `--view`,
`--info`, `--animate_sharding`, repeated `-o`, and `-o -` (stdout). Detail below.

### G2 — Post-2021.01 & experimental language surface

- **String escapes.** `crates/openrscad-syntax/src/lexer.rs:6-31` handles only
  `\n \t \r \\ \"`; `\u`, `\U`, `\x`, `\0` fall through as literal backslash
  sequences. The manifest's `value.string` entry does not cover escapes.
- **`fill()`** — not implemented; hits `Ignoring unknown module`
  (`crates/openrscad-eval/src/lib.rs:1237`). 2D-only; a natural fit for
  `shape2d.rs` (drop holes = keep outer rings from `group_contours`).
- **Object values** — `Value::Object` already exists (`value.rs:29`) with `.field`
  and `["key"]` access, but is only *constructible* by `textmetrics`/`fontmetrics`.
  Add the source-level constructor plus `is_object`/`has_key`/`keys()`/`values()`.
- **`lazy-union`**, **import-as-a-function**.
- Method: audit each against the public manual first, add manifest entries at
  `current_stable` or an experimental tier, *then* implement. Do not guess syntax.

### G3 — Geometry completeness

The two loud holes, both already warned at runtime:
- **`roof()`** is edge-events-only (`crates/openrscad-geom/src/roof.rs:23`);
  concave or holed profiles emit `ROOF_UNSUPPORTED_WARNING` and **no geometry**.
  Needs a general straight skeleton (split events + multi-ring).
- **3D `minkowski()`** on a genuinely concave leaf falls back to the convex hull
  (`geom/lib.rs:1517,1530`). Track F's open F-G7. Needs bounded convex
  decomposition — the roadmap calls this "research-grade, ~2 weeks."

Smaller, cheap, and currently undocumented:
- SVG import ignores `$fn/$fa/$fs` — hard-coded `CURVE_STEPS = 24`
  (`vector2d.rs:348`); `import_svg` doesn't even take a `FragmentSpec`. DXF
  honors it. Asymmetric and not in COMPAT.md.
- `text()` flattens glyph curves from `$fn` only (`eval/lib.rs:1742`); `$fa`/`$fs`
  have no effect.
- Narrow positional-arg lists silently drop arguments: `b_linear_extrude` binds
  only `["height"]`, `b_rotate_extrude` only `["angle"]`, `b_offset` only
  `["r","delta"]` — so `linear_extrude(10, true)` and `offset(1, false, true)`
  lose `center`/`chamfer`. Untested in the corpus; verify against the oracle.
- wasm `hull()` uses a hand-rolled incremental hull (`geom/src/hull.rs`, 178
  lines, self-described as "the common cases"), not Manifold's. Every 3D
  `minkowski()` depends on it on both backends.

### G4 — Library parity

`xtask bosl2` runs only `[[test]]` **function** blocks over a 15-file subset
(`xtask/src/main.rs:60-80`). BOSL2's real surface — `attachments`, `rounding`,
`skin`, `gears`, `screws`, `threading` — is module/geometry-level and untested.
Extend the oracle to geometry blocks (reuse `metrics()` from the geom harness),
then broaden to other common libraries (MCAD, NopSCADlib, Round-Anything,
dotSCAD, threads.scad). This is the strongest practical parity signal we don't
yet have.

### G5 — GUI parity (own workstream)

Against OpenSCAD's View/Design/Edit menus and Preferences, we lack:
- **View:** standard camera presets (Top/Bottom/Left/Right/Front/Back/Diagonal,
  View All, Center), **Wireframe**, **Thrown Together**, show crosshairs, show
  scale markers. We have grid/axes/edges/orthographic/section already.
- **Viewport color schemes** — Cornfield, Metallic, Sunset, Starnight,
  BeforeDawn, Nature, DeepOcean, Tomorrow (+ Night), Monotone. We have only
  light/dark app themes.
- **Editor:** find/replace, comment/uncomment toggle, indent/unindent, code
  folding, bracket matching, editor color schemes. CodeMirror 6 supplies most of
  these as packages; wire them into `web/src/lang/` + the command registry.
- **Dialogs/panels:** Preferences pane, Font List, Library Info, Display CSG
  Tree / CSG Products, Check Validity. `.csg` export and the Objects tree
  already cover part of this.
- **Design:** Automatic Reload and Preview toggle (desktop already watches
  files), 3D print service beyond macOS-only "Open in" (`openwith.rs` returns
  empty on every non-macOS platform).
- Every entry must land in `web/src/commands/index.ts` — the single `COMMANDS`
  array that the topbar, palette, help sheet, shortcuts, and native menu are all
  projections over (Track E's invariant: controls must stay countable).

### G6 — Editor/LSP parity + compat-surface de-duplication

- LSP is missing goto-definition, find-references, rename, formatting, semantic
  tokens, signature help, folding. `analyze::Symbol` already carries a `span`
  "for go-to and document symbols" (`analyze.rs:22`) — the data exists, the
  handlers don't.
- **Quick win, already flagged P1 in Track F:** the builtin list is duplicated
  across evaluator dispatch, `crates/openrscad-lsp/src/builtins.rs` (~66 names)
  and `web/src/lang/builtins.ts` (80 names), and the tables **have drifted** —
  both omit `rands, version, version_num, is_range, surface, roof, group,
  assign, child, import_stl, import_dxf, dxf_dim, dxf_cross, $children`.
  Generate all three from one source of truth and assert coverage in a test.

---

## Batch 1 — G1 in detail (implement now)

Ordered so each step is independently testable. All in
`crates/openrscad-cli/src/main.rs` unless noted.

**1. Flags that are pure additions to `Cli`**
- `--enable <FEATURE>` (repeatable) — **accepted and ignored**, per the
  always-on decision. Prevents `--enable=manifold` / `--enable=roof` from
  breaking upstream scripts. Document in `--help` that features are always on.
- `--csglimit <N>` — accepted and ignored (we have no OpenCSG preview); note it
  in `COMPAT.md` rather than warning, since ignoring it is behaviorally correct.
- `-q, --quiet` — suppress echoes, warnings, and the stats block; errors still
  go to stderr and still set the exit code.
- `--hardwarnings` — exit non-zero if `EvalOutput.warnings`
  (`eval/lib.rs:265`) is non-empty, after printing them.
- `--info` — print version, kernel backend, enabled features, font dirs,
  `OPENSCADPATH` entries; exit 0.

**2. Output plumbing rework** (`OutputFormat`, `main.rs:157-212`)
- `--export-format <FMT>` — explicit format, overriding suffix inference.
  Accept upstream spellings incl. `binstl`/`asciistl`/`echo`, mapping `echo` to
  the existing `--check` path. Enumerate the accepted set from
  `openscad --help` on the oracle machine before finalizing; ship the subset we
  can actually write and reject the rest with a clear message.
- `-o -` → stdout. Requires `--export-format` (no suffix to infer from);
  binary formats write raw bytes to stdout, and `-q` becomes implicit so stats
  don't corrupt the stream.
- Repeated `-o` — change `output: Option<PathBuf>` to `Vec<PathBuf>`, render
  once and write each. Keep the current *pre-flight* suffix validation for
  every path (the "don't throw away a long render on a typo" property at
  `main.rs:184-191`).
- `--projection` — accept `o`/`orthogonal`/`p` as aliases (`Proj` enum).

**3. Two new export formats** (`crates/openrscad-geom/src/mesh.rs`,
`vector2d.rs`)
- **WRL / VRML2** — `to_wrl()` next to `to_off`/`to_obj`: `#VRML V2.0 utf8`
  header, one `Shape { geometry IndexedFaceSet {…} }` per color group so `color()`
  survives, mirroring how `to_3mf_colored` (`mesh.rs:329`) already partitions.
- **PDF** — `export_pdf(contours)` next to `export_svg` (`vector2d.rs:1285`):
  2D vector output, minimal uncompressed PDF (catalog, pages, one content
  stream of `m`/`l`/`h`/`f*` path ops). Same "requires a 2D object" error path
  as DXF/SVG.
- Add both to `OutputFormat`, the web/desktop export menus
  (`web/src/renderState.ts:19-20`, `desktop/src-tauri/src/lib.rs:493-580`), and
  the manifest as `export.wrl` / `export.pdf` at status `implemented`.

**4. Dependency output — `-d/--deps_file <FILE>`, `-m/--make <CMD>`**
- Give `DiskResolver` (`main.rs:13-40`) a `RefCell<Vec<PathBuf>>` recording every
  successfully resolved path in **both** `load` and `load_bytes` — so
  `import()`, `surface()`, and DXF/SVG fixtures land in the deps list too, not
  just `include`/`use`. `&self` methods + single-threaded use inside the worker
  make `RefCell` sufficient; no change to the `FileResolver` trait or to
  `EvalOutput`.
- Emit `<output>: <input> <dep> <dep>…` Make syntax. `-m` supplies the command
  to rebuild a missing dependency.

**5. `-D` accepts arbitrary expressions**
Today `-D` goes through `customizer::parse_value`
(`crates/openrscad-syntax/src/customizer.rs:340`), which handles only a number,
bool, quoted string, or **flat** numeric vector — so `-D 'm=[[1,2],[3,4]]'`
fails, and OpenSCAD's `-D` takes any expression. Add
`openrscad_eval::eval_const_expr(&str) -> Result<Value>` (parse an expression,
evaluate against the base scope so `PI`, `sqrt()`, etc. work), and use it for
`-D` with the current parser as the fallback. Keep customizer parsing unchanged
— `-p`/`-P` JSON sets are a different, typed surface.

**6. Parameter-set validation**
- `--check-parameters` / `--check-parameter-ranges` — validate a `-p` set
  against the customizer schema (`customizer::extract`, `customizer.rs:179`),
  reporting unknown names and out-of-range slider/dropdown values.
- Relax the current hard error at `main.rs:392`: OpenSCAD tolerates `-p`
  without `-P`. Keep `-P` without `-p` an error.

**7. `--summary <SET>` / `--summary-file <FILE>`**
Accept the documented comma-separated set (`all,cache,time,camera,geometry,
bounding-box,area,volume`). The existing stats block already computes
tris/verts/time/volume/area; restructure it into a `Summary` struct rendered
either as OpenSCAD-style text or as JSON to `--summary-file`.

**8. PNG parity**
- **`$vpr`/`$vpt`/`$vpd`/`$vpf` drive the camera.** Real bug: `EvalOutput.viewport`
  (`eval/lib.rs:271`) is already populated and the *web* viewer consumes it
  (`App.tsx:1909-1911`), but `grep viewport crates/openrscad-cli/` returns
  nothing — the CLI raster honors only `--camera`/`--viewall`/`--autocenter`.
  Feed a script-set viewport into `raster::Camera::Gimbal` when `--camera` is
  absent. Highest-value item in the batch: today, a script that sets its own
  camera renders from the wrong angle.
- `--colorscheme <NAME>` — map names to `RenderOpts.background` + face colors
  (`raster.rs:44-60`).
- `--view <axes,scales,edges,wireframe,crosshairs>` — overlays in `raster.rs`.
- `--animate_sharding <i>/<n>` — render only frames `≡ i (mod n)` in
  `run_animation` (`main.rs:288`).

**9. Bookkeeping**
- Add a `cli` category to `compatibility/openscad-2021.01.json` (there is none
  today) with one entry per flag; `compatibility/validate.py` must still pass.
- Update `COMPAT.md` for accepted-and-ignored flags (`--enable`, `--csglimit`)
  and any divergence in `--summary` formatting.
- One `patch` changeset (bug-fix-shaped) — but if the `-o` signature change or
  WRL/PDF is judged new capability, `minor`. Per `CLAUDE.md`, `minor` costs the
  caret boundary, so default to `patch` unless the `-o` `Vec` change breaks a
  documented invocation.

## Verification

Existing gates must stay green — these are the release contract:

```
cargo fmt --check && cargo clippy --workspace -- -D warnings
cargo test --workspace                      # 277 unit + integration tests
cargo run -p xtask -- echo                  # 35/35
cargo run -p xtask -- geom                  # 130/130
cargo run -p xtask -- warm-gate             # warm re-render < 20 ms
python3 compatibility/validate.py
```

(`xtask bosl2` needs the `corpus/BOSL2` submodule, which is **not checked out**
in this worktree — `git submodule update --init corpus/BOSL2` first, or skip and
let CI run it.)

New coverage for batch 1:

- **CLI integration tests** in a new `crates/openrscad-cli/tests/flags.rs`,
  alongside the existing `csg_roundtrip.rs` / `provenance_hierarchy.rs`. One
  test per flag, driving the built binary: `--hardwarnings` exit code, `-q`
  silence, `-d` deps content (assert an `import()`ed fixture appears, not just
  the `include`), `-o -` byte-for-byte equal to `-o file`, repeated `-o`,
  `--export-format` overriding a mismatched suffix, `-D 'm=[[1,2],[3,4]]'` and
  `-D 'r=sqrt(2)*2'`, `--check-parameters` catching an out-of-range set,
  `--enable=roof` being a no-op.
- **Round-trip tests** for WRL and PDF in `mesh.rs` / `vector2d.rs` unit tests,
  matching how the other writers are covered.
- **`$vp*` camera:** render the same model twice — once with a script-set
  `$vpr`, once with the equivalent `--camera` — and assert the PNGs match. This
  is the assertion that proves the bug is fixed.
- **Manual smoke:** `openrscad examples/lamp.scad -o /tmp/l.wrl`,
  `-o /tmp/l.pdf` (from a 2D model), `-o - --export-format binstl | wc -c`,
  and a Makefile using `-d` to confirm the dependency graph actually rebuilds.

## Suggested sequencing after batch 1

G6's builtin-table de-duplication (cheap, already a flagged P1, fixes real
missing autocompletions) → G2 language surface (small, self-contained; needs a
manual audit first) → G5 GUI (broad but low-risk) → G4 library oracle (infra;
needs the oracle machine) → G3 geometry (research-grade; do last).
