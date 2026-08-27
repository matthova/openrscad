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
| `xtask echo` | 34/34 | selected expression, scope, comprehension, builtin, and file-resolution behavior | complete syntax/builtin/diagnostic coverage |
| `xtask geom` | 122/122 | selected mesh metrics vs OpenSCAD 2024.12 | every parameter, all vector output, both kernels, or byte-identical meshes |
| `xtask bosl2` | 505/513 blocks | broad real-library function behavior | all BOSL2 modules or the eight expected failures |
| Rust workspace tests | 265 tests | local invariants and host integration | upstream equivalence |

The executable manifest currently classifies 190 surfaces: 174 `verified`, 11
`implemented` but not oracle-proven, 1 `missing`, one warned divergence, and
three permanent divergences. Of those, 185 belong to the 2021.01 core or its
retained deprecated surface. Every confirmed difference is decomposed into a
measured repro in the [compatibility atom register](../compat-atoms.md).

The 11 that are still only `implemented` are blocked structurally rather than by
effort: nine export formats, whose only `.scad`-level comparison would be byte
identity (explicitly outside the contract); `rotate_extrude(start=)`, which the
selected oracle predates; and `is_range()`, an OpenRSCAD extension the oracle
reports as unknown. Everything else in the 2021.01 core is now `verified` or a
documented divergence.

Proving the other 39 was not bookkeeping. It found three real differences that
no corpus case had ever touched: `$vpf` defaulting to 45 instead of 22.5, PNG
`surface(invert=true)` inverting against 255 where upstream inverts against 1,
and a mis-wound `polyhedron` that *subtracted* itself from a union instead of
adding (and cut nothing at all as a difference tool). That last one is the
strongest evidence for the `implemented` status existing at all.

The geometry oracle compares volume (±0.1%), bounding box and centroid
(±0.01 mm), components, and manifoldness. It does **not** compare bytes, vertex
ordering, or every triangle. Product claims must describe it that way.

## Confirmed remaining gaps

These are observed differences, not speculative TODOs. Repros were run against
OpenSCAD 2024.12.17 unless marked as an audit/coverage item.

### P0 — silent program or geometry changes

_All P0 silent geometry gaps are now closed and oracle-gated._

- ~~F-G3 `linear_extrude` twist combined with a non-uniform scale.~~ **Closed.**
  Each edge is refined by the peak stretch its direction reaches over the swept
  slices (`max_t |diag(sx(t),sy(t))·Rot(t·twist)·d|`) — a rule that subsumes the
  pure-twist and pure-scale ones — and the layer sweep rotates then scales in the
  fixed frame. The headline case is exact (246.445, 1244 tris; was +0.77%). Gated
  by `corpus/geom/ext_linear_twist_scale{,_neg,_hole}.scad`; see A-G11 in
  `docs/compat-atoms.md`.

### P1 — missing core surface or broad fidelity work

| id | gap | scope |
|---|---|---|
| F-G7 | 3D Minkowski of a genuinely concave leaf remains a warned convex approximation. | Keep it loud and permanent, or implement bounded convex decomposition; never silently relabel it exact. |

### P1 — measurement gaps

- The compatibility surface is duplicated between evaluator dispatch, Rust LSP
  completions, and web completions. The tables already drift: implemented
  `surface`, `rands`, `version`, and other entries are absent or incomplete.
- Geometry goldens exercise the native C++ kernel and 3D mesh metrics. They do
  not yet oracle 2D contours/DXF/SVG bytes or diagnostics. The Rust/Wasm kernel
  is covered by differential and targeted unit tests, not by the corpus, so a
  backend-specific fix needs its own test — as the winding repair did.

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

1. ~~F-G3 `linear_extrude` refinement.~~ Complete. The twist rules (`segments=`,
   implicit `slices`, profile re-tessellation), the non-uniform scale, the
   wall diagonal, and the twist + non-uniform-scale combination are all
   oracle-gated.
2. ~~F-G4 `rotate_extrude` side/sweep rules.~~ Complete.
3. ~~F-G6 kernel-aware vector projection export.~~ Complete across CLI, wasm/npm,
   LSP, and desktop.
4. ~~F-G5 concave-face triangulation.~~ Complete.
5. Decide F-G7 exact decomposition vs permanent warned divergence.

Every geometry commit adds an OpenSCAD golden and passes both native and
Rust/Wasm kernel tests. No volume-changing fix lands with only a unit test.

### F3 — text and file-format conformance

1. ~~Shape text (including kerning/ligatures), then add complex and vertical
   scripts; preserve the bundled Liberation fallback for deterministic models.~~
   Complete, via `rustybuzz`.
2. ~~Implement import arguments/selectors independently of parser breadth.~~
   Complete.
3. ~~Add SVG transforms/structure and DXF curves/layers.~~ Complete.
4. ~~Assemble 3MF/AMF objects with units/transforms/components.~~ Complete;
   3MF multi-object assembly is a recorded deliberate divergence.

Use small authored fixtures and exported metrics. Do not import implementation
details or tests from OpenSCAD source.

### F5 — clear the unproven class

~~Promote every `implemented` 2021.01 core entry to `verified` or to a documented
divergence.~~ Complete except for entries no `.scad` oracle can reach (the nine
export formats, `rotate_extrude(start=)`, and the `is_range` extension). This
pass added 13 oracle cases, an `// oracle: libs <dir>` directive so the harness
can put a directory on the oracle's `OPENSCADPATH` and prove library resolution,
and fixed the three differences it uncovered (`$vpf`, PNG `invert`, polyhedron
winding).

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

Passing 122/122 or 505/513 remains useful evidence, but completion is defined by
the classified surface, not by freezing those counts.

**Where this stands:** the first criterion is met apart from the F-G7 decision;
there are zero `missing` entries and no known silent differences, and every
remaining `implemented` entry has a stated structural reason. The open item is
F-G7, not a backlog of unmeasured surface.

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
