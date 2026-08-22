# Track F — M10 "Measured OpenSCAD Compatibility"

**Status:** active. Audit started 2026-08-09 at `2368691` against the public
OpenSCAD manual and the installed OpenSCAD 2024.12.17 binary. This is a
clean-room plan; no OpenSCAD source is used.

**Goal:** turn "OpenSCAD compatible" from a broad intent into an executable
contract. Every documented language feature and core modeling parameter must be
classified, every supported classification must point to an oracle or regression
test, and every known difference must either be fixed or be visible at runtime.

Track A and the first Track F fixes prove 81 representative geometry models and
25 language scripts. That is valuable regression coverage, but it is not an
inventory of the whole OpenSCAD surface. The audit for this track found
unsupported constructs and silently ignored parameters outside that corpus, so
passing those suites cannot yet mean full compatibility.

## Compatibility contract

Upstream compatibility has two tiers so the target is stable without freezing
the project in 2021:

1. **Core baseline — OpenSCAD 2021.01.** Stable language syntax, values,
   functions, modules, special variables, and file semantics documented for
   2021.01 are release-blocking. The 2024.12.17 binary may be used as the oracle
   where that behavior remains supported.
2. **Current stable tier.** Stable features added after 2021.01 are tracked
   individually and may land incrementally. The exact upstream version first
   supporting each feature belongs in the manifest described below.

OpenRSCAD-only additions carry a separate `openrscad_extension` label. They are
inventoried to prevent accidental compatibility claims, but are excluded from
both upstream tiers and the OpenSCAD exit criterion.

Experimental features (`roof`, object values, import-as-a-function,
`textmetrics`, and similar feature-flagged surfaces), GUI pixel parity, identical
triangle ordering, and reproduction of CGAL bugs are not part of the core exit
criterion. CLI workflow parity is tracked separately from `.scad` program
semantics so a missing export format cannot be confused with wrong model
geometry.

Each manifest entry has exactly one status:

- **verified** — implemented and linked to an OpenSCAD oracle case;
- **implemented** — present, but not yet proven across its documented inputs;
- **missing** — rejected or ignored today;
- **warned divergence** — intentionally different and surfaced to the user;
- **permanent divergence** — intentionally different, with rationale and a
  minimal repro in `COMPAT.md`;
- **unknown** — inventoried but not yet audited.

"Implemented" is deliberately not a synonym for "compatible."

## Evidence already in place

| evidence | current result | what it establishes | what it does not establish |
|---|---:|---|---|
| `xtask echo` | 29/29 | selected expression, scope, comprehension, and builtin behavior | complete syntax/builtin/diagnostic coverage |
| `xtask geom` | 100/100 | selected mesh metrics vs OpenSCAD 2024.12 | every parameter, all vector output, both kernels, or byte-identical meshes |
| `xtask bosl2` | 505/513 blocks | broad real-library function behavior | all BOSL2 modules or the eight expected failures |
| Rust workspace tests | 234 tests | local invariants and host integration | upstream equivalence |

The executable manifest currently classifies 189 surfaces: 125 `verified`, 51
`implemented` but not yet oracle-proven, 11 `missing`, one warned divergence,
and one permanent divergence. Of those, 184 belong to the 2021.01 core or its
retained deprecated surface. Every confirmed difference is decomposed into a
measured repro in the [compatibility atom register](../compat-atoms.md).

The geometry oracle compares volume (±0.1%), bounding box and centroid
(±0.01 mm), components, and manifoldness. It does **not** compare bytes, vertex
ordering, or every triangle. Product claims must describe it that way.

## Confirmed remaining gaps

These are observed differences, not speculative TODOs. Repros were run against
OpenSCAD 2024.12.17 unless marked as an audit/coverage item.

### P0 — silent program or geometry changes

| id | gap | current effect | first fix |
|---|---|---|---|
| F-G3 | `linear_extrude` twist combined with a non-uniform scale. | The twist rules, the non-uniform-scale rules, and the non-planar wall diagonal are all closed and oracle-gated. What remains is only their combination, which weights the profile edges the opposite way upstream (+0.8% volume). | Identify the combined weighting with the per-quad harness that settled the diagonal, applied to segment counts. |

### P1 — missing core surface or broad fidelity work

| id | gap | scope |
|---|---|---|
| F-I1 | Text is not fully shaped. | System fonts are now discoverable, but layout is codepoint-by-codepoint: no kerning, ligatures, complex-script shaping, vertical text, or meaningful `language`/`script`; RTL only reverses codepoints. Use a shaping library and test Latin + RTL + vertical cases. |
| F-I2 | DXF/SVG import is a useful subset, not format parity. | Layer/id selection, import transforms/DPI, SVG group/element transforms, `<use>`, style/visibility, DXF bulges/splines/ellipses, and caller fragment controls are missing. Split selectors, SVG structure, and DXF curves into separate commits. |
| F-I3 | 3MF/AMF import flattens XML tags rather than a scene graph. | Units, per-object index bases, build-item transforms/components, and multi-object assembly can be wrong. Implement scene/object assembly before materials. |
| F-I4 | OpenSCAD-style `.csg` tree export is absent. | Serialize the evaluated operation tree and add CLI/export round-trip fixtures. Unknown-suffix handling is fixed (F-X1 closed), so `.csg` now fails loudly instead of silently writing STL — this is a missing feature, no longer a silent one. |
| F-G7 | 3D Minkowski of a genuinely concave leaf remains a warned convex approximation. | Keep it loud and permanent, or implement bounded convex decomposition; never silently relabel it exact. |

### P1 — measurement gaps

- The compatibility surface is duplicated between evaluator dispatch, Rust LSP
  completions, and web completions. The tables already drift: implemented
  `surface`, `rands`, `version`, and other entries are absent or incomplete.
- Geometry goldens exercise the native C++ kernel and 3D mesh metrics. They do
  not yet oracle 2D contours/DXF/SVG bytes, diagnostics, or the Rust/Wasm kernel.
- Import fixtures are deliberately simple. They do not exercise scene graphs,
  units, transforms, layers, or compound paths.

## Execution plan

### F0 — make the contract executable

1. ~~Correct stale compatibility claims and make this track the active roadmap.~~
   Complete.
2. ~~Add one machine-readable manifest covering syntax, evaluator, geometry,
   import/export, CLI, and host surfaces. Include version tier, status, supported
   hosts, repro, and test/oracle IDs.~~ Complete.
3. ~~Generate the human summary from that manifest, or at minimum validate in CI
   that every `verified` entry has a test and every divergence has a repro.~~
   CI validation is complete; generated summaries remain optional follow-up.
4. ~~Key BOSL2 blocks by file + ordinal + name and store an explicit
   expected-failure manifest. A changed total, duplicate, regression, or newly
   passing block must be reported unambiguously.~~ Complete.

### F1 — close core language and argument binding

Land one behavior family per commit, each with a black-box oracle repro:

1. ~~F-G1 builtin signature fixes and a table-driven binding test.~~ Initial
   confirmed cases are fixed; the manifest-wide audit remains.
2. ~~F-L1 default-expression scope/laziness in tree-walk and VM paths.~~ Complete.
3. ~~F-L2 `$preview` propagation across every host/export path.~~ Complete.
   Evaluation mode is an explicit `RenderMode` chosen per host; both directions
   are oracle-gated (`geom:preview_branch` exact, `echo:preview_mode` preview).
4. ~~F-L3/F-L4 missing baseline constructs.~~ Complete.
   ~~F-L7 `$` arguments dynamically scoped to a call's children.~~ Complete for
   builtin modules, user modules, and function calls; retired two BOSL2
   expected failures.
5. ~~F-L5 scalar/list/builtin edge semantics~~ and ~~F-G2 primitive
   validation~~. Complete: a non-positive dimension now yields an empty node,
   which also stopped degenerate primitives from failing enclosing booleans.

### F2 — close geometry operations

1. ~~F-G3 `linear_extrude` refinement.~~ The twist rules (`segments=`, implicit
   `slices`, and profile re-tessellation) are complete and oracle-gated; the
   wall-diagonal and non-uniform-scale remainders are re-scoped above.
2. ~~F-G4 `rotate_extrude` side/sweep rules.~~ Complete.
3. ~~F-G6 kernel-aware vector projection export.~~ Complete across CLI, wasm/npm,
   LSP, and desktop.
4. ~~F-G5 concave-face triangulation.~~ Complete.
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

Passing 100/100 or 505/513 remains useful evidence, but completion is defined by
the classified surface, not by freezing those counts.

## Completed during the initial Track F pass

- F-G1 confirmed argument holes: named `multmatrix`, fourth positional
  `cylinder` argument, and the full positional `text` signature.
- F-G4 `rotate_extrude` side, sweep, fragment, and over-rotation semantics.
- F-G6 bare/display-wrapped projection export to DXF/SVG on every first-party
  host.
- F-L3/F-L4 `intersection_for`, `$parent_modules`, and `parent_module()`.
- F-L5 NaN/index/iteration, strict numeric reducers, `chr`, and `version_num`
  edge semantics.
- F-L1 lazy parameter defaults in lexical definition scope, with supplied
  arguments evaluated in caller scope and `$` variables kept dynamic.
- F-G5 projected triangulation for general planar concave `polyhedron` faces.
- Raw punctuation/space-preserving `include` and `use` paths.
- BOSL2 block identities now use file + ordinal + name, pin exactly 513 blocks,
  and explicitly gate every expected failure. That list was ten at the time and
  is now eight: the two `test_segs` blocks began passing once `$` arguments were
  scoped to a call's children.
