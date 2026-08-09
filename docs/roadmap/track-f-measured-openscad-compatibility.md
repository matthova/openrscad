# Track F — M10 "Measured OpenSCAD Compatibility"

**Status:** active. Audit started 2026-08-09 at `2368691` against the public
OpenSCAD manual and the installed OpenSCAD 2024.12.17 binary. This is a
clean-room plan; no OpenSCAD source is used.

**Goal:** turn "OpenSCAD compatible" from a broad intent into an executable
contract. Every documented language feature and core modeling parameter must be
classified, every supported classification must point to an oracle or regression
test, and every known difference must either be fixed or be visible at runtime.

Track A proved 76 representative geometry models and 25 language scripts. That
is valuable regression coverage, but it is not an inventory of the whole
OpenSCAD surface. The audit for this track found unsupported constructs and
silently ignored parameters outside that corpus, so passing those suites cannot
yet mean full compatibility.

## Compatibility contract

Compatibility has two tiers so the target is stable without freezing the
project in 2021:

1. **Core baseline — OpenSCAD 2021.01.** Stable language syntax, values,
   functions, modules, special variables, and file semantics documented for
   2021.01 are release-blocking. The 2024.12.17 binary may be used as the oracle
   where that behavior remains supported.
2. **Current stable tier.** Stable features added after 2021.01 are tracked
   individually and may land incrementally. The exact upstream version first
   supporting each feature belongs in the manifest described below.

Experimental features (`roof`, object values, import-as-a-function,
`textmetrics`, and similar feature-flagged surfaces), GUI pixel parity, identical
triangle ordering, and reproduction of CGAL bugs are not part of the core exit
criterion. CLI workflow parity is tracked separately from `.scad` program
semantics so a missing export format cannot be confused with wrong model
geometry.

Each manifest entry will have exactly one status:

- **verified** — implemented and linked to an OpenSCAD oracle case;
- **implemented** — present, but not yet proven across its documented inputs;
- **missing** — rejected or ignored today;
- **warned divergence** — intentionally different and surfaced to the user;
- **permanent divergence** — intentionally different, with rationale and a
  minimal repro in `COMPAT.md`.

"Implemented" is deliberately not a synonym for "compatible."

## Evidence already in place

| evidence | current result | what it establishes | what it does not establish |
|---|---:|---|---|
| `xtask echo` | 25/25 | selected expression, scope, comprehension, and builtin behavior | complete syntax/builtin/diagnostic coverage |
| `xtask geom` | 76/76 | selected mesh metrics vs OpenSCAD 2024.12 | every parameter, 2D vector export, both kernels, or byte-identical meshes |
| `xtask bosl2` | 503/513 blocks | broad real-library function behavior | all BOSL2 modules or the ten expected failures |
| Rust workspace tests | 201 tests at audit time | local invariants and host integration | upstream equivalence |

The geometry oracle compares volume (±0.1%), bounding box and centroid
(±0.01 mm), components, and manifoldness. It does **not** compare bytes, vertex
ordering, or every triangle. Product claims must describe it that way.

## Confirmed remaining gaps

These are observed differences, not speculative TODOs. Repros were run against
OpenSCAD 2024.12.17 unless marked as an audit/coverage item.

### P0 — silent program or geometry changes

| id | gap | current effect | first fix |
|---|---|---|---|
| F-L1 | Function/module defaults use the caller's lexical scope and are evaluated even when overridden. | A default can read the wrong value or execute an `echo`/assert that OpenSCAD never evaluates. | Bind supplied arguments first; evaluate only missing defaults in the closure's lexical environment while retaining dynamic `$` variables. Cover tree-walk and VM paths. |
| F-L2 | `$preview` is seeded `true` for every native evaluation. | Exact renders and exports can take a script's preview-only branch. | Make evaluation mode explicit: true only for fast/F5-style preview, false for exact render/export. Test CLI, wasm, desktop, and npm paths. |
| F-G1 | Builtin argument signatures are incomplete. | `multmatrix(m=...)` becomes identity; `cylinder(10,5,3,true)` ignores `center`; later positional `text(...)` arguments are dropped. | Centralize documented signatures and test positional, named, and mixed binding for every builtin. |
| F-G2 | Invalid primitive dimensions are accepted. | Negative cube/square dimensions, cylinder height/radii, and extrusion height produce solids where OpenSCAD produces no geometry. | Add shared validation, matching warnings, and empty-node behavior with oracle cases. |
| F-G3 | `linear_extrude` omits documented refinement behavior. | `segments` is ignored and omitted `slices` does not follow `$fn`; a twisted `$fn=40` repro differs by about 14% in volume. | Implement the documented slice/refinement rules and add twist × `$fn` × `segments` corpus cases. |
| F-G4 | `rotate_extrude` mishandles negative-X profiles, profiles crossing the axis, partial sweeps, and angles over 360°. | It can collapse resolution, create a zero-volume surface instead of an error, or over-tessellate/double-sweep. | Normalize the profile side and sweep, reject axis crossings, and scale fragments by sweep angle. |
| F-G5 | General `polyhedron` faces are fan-triangulated. | A concave, non-star-shaped face can be filled incorrectly. | Project each planar face and triangulate it with earcut; validate indices/coplanarity. |
| F-G6 | Bare `projection()` is dropped by DXF/SVG export. | The viewport can show the projection while vector export writes an empty shape. | Expose kernel-aware projection lowering to contour export and gate CLI + wasm output against OpenSCAD. |

### P1 — missing core surface or broad fidelity work

| id | gap | scope |
|---|---|---|
| F-L3 | `intersection_for(...)` is advertised by completion but evaluated as an unknown module. | Implement its Cartesian/dependent bindings and intersection semantics. |
| F-L4 | `$parent_modules` and `parent_module(i)` are absent. | Track the dynamic module-instantiation stack and oracle nested/children/include calls. |
| F-L5 | Several value/builtin edge semantics differ. | NaN truthiness/indexing, `for(i=undef)`, invalid values in `min`/`max`/`norm`, multi-argument `chr`, and `version_num(v)` all have confirmed small repros. |
| F-L6 | Some legal include/use paths fail during lexing, and retained deprecated 2021-era aliases are unsupported. | Add raw angle-path lexing; decide and record support for `assign`, `child`, `import_dxf`, `import_stl`, `dxf_dim`, and `dxf_cross`. |
| F-I1 | Text is not fully shaped. | System fonts are now discoverable, but layout is codepoint-by-codepoint: no kerning, ligatures, complex-script shaping, vertical text, or meaningful `language`/`script`; RTL only reverses codepoints. Use a shaping library and test Latin + RTL + vertical cases. |
| F-I2 | DXF/SVG import is a useful subset, not format parity. | Layer/id selection, import transforms/DPI, SVG group/element transforms, `<use>`, style/visibility, DXF bulges/splines/ellipses, and caller fragment controls are missing. Split selectors, SVG structure, and DXF curves into separate commits. |
| F-I3 | 3MF/AMF import flattens XML tags rather than a scene graph. | Units, per-object index bases, build-item transforms/components, and multi-object assembly can be wrong. Implement scene/object assembly before materials. |
| F-G7 | 3D Minkowski of a genuinely concave leaf remains a warned convex approximation. | Keep it loud and permanent, or implement bounded convex decomposition; never silently relabel it exact. |

### P1 — measurement gaps

- The compatibility surface is duplicated between evaluator dispatch, Rust LSP
  completions, and web completions. The tables already drift: completions offer
  missing `intersection_for`, while implemented `surface`, `rands`, `version`,
  and other entries are absent or incomplete.
- The BOSL2 baseline keys passing blocks by deduplicated name. The pinned suite
  has two `test_segs` blocks, so one can regress while the other masks it. The ten
  expected failures are nine distinct names: `test_gaussian_rands`,
  `test_format`, `test_format_float`, `test_str_strip`, `test_hstack`,
  `test_typeof`, two `test_segs` blocks, `test_f_acos`, and `test_struct_val`.
- Geometry goldens exercise the native C++ kernel and 3D mesh metrics. They do
  not yet oracle 2D contours/DXF/SVG bytes, diagnostics, or the Rust/Wasm kernel.
- Import fixtures are deliberately simple. They do not exercise scene graphs,
  units, transforms, layers, or compound paths.

## Execution plan

### F0 — make the contract executable

1. Correct stale compatibility claims and make this track the active roadmap.
2. Add one machine-readable manifest covering syntax, evaluator, geometry,
   import/export, CLI, and host surfaces. Include version tier, status, supported
   hosts, repro, and test/oracle IDs.
3. Generate the human summary from that manifest, or at minimum validate in CI
   that every `verified` entry has a test and every divergence has a repro.
4. Key BOSL2 blocks by file + ordinal + name and store an explicit
   expected-failure manifest. A changed total, duplicate, regression, or newly
   passing block must be reported unambiguously.

### F1 — close core language and argument binding

Land one behavior family per commit, each with a black-box oracle repro:

1. F-G1 builtin signature fixes and a table-driven binding test.
2. F-L1 default-expression scope/laziness in tree-walk and VM paths.
3. F-L2 `$preview` propagation across every host/export path.
4. F-L3/F-L4 missing baseline constructs.
5. F-L5 scalar/list/builtin edge semantics and F-G2 primitive validation.

### F2 — close geometry operations

1. F-G3 `linear_extrude` refinement.
2. F-G4 `rotate_extrude` side/sweep rules.
3. F-G6 kernel-aware vector projection export.
4. F-G5 concave-face triangulation.
5. Decide F-G7 exact decomposition vs permanent warned divergence.

Every geometry commit adds an OpenSCAD golden and passes both native and
Rust/Wasm kernel tests. No volume-changing fix lands with only a unit test.

### F3 — text and file-format conformance

1. Shape text (including kerning/ligatures), then add complex and vertical
   scripts; preserve the bundled Liberation fallback for deterministic models.
2. Implement import arguments/selectors independently of parser breadth.
3. Add SVG transforms/structure and DXF curves/layers.
4. Assemble 3MF/AMF objects with units/transforms/components.

Use small authored fixtures and exported metrics. Do not import implementation
details or tests from OpenSCAD source.

### F4 — optional compatibility tails

After the 2021.01 manifest is fully classified, decide deprecated aliases,
post-2021 stable features, and CLI export/flag parity as explicit batches.
Experimental features stay outside the baseline unless promoted upstream.

## Exit criterion

M10 is complete only when all of the following are CI-enforced:

- every 2021.01 core manifest entry is `verified`, a warned divergence, or a
  justified permanent divergence — none remain merely `implemented` or missing;
- there are zero known silent differences; all fallback/degraded paths reach
  diagnostics in CLI, desktop, web, LSP, and npm;
- the echo, geometry, 2D-vector, diagnostics, and BOSL2 gates identify their
  exact upstream target and expected divergences;
- geometry compatibility cases run through both kernels used by products;
- `COMPAT.md`, completion metadata, and the manifest cannot drift without a CI
  failure.

Passing 76/76 or 503/513 remains useful evidence, but completion is defined by
the classified surface, not by freezing those counts.
