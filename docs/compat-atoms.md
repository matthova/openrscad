# Compatibility atoms

An **atom** is the smallest independently closable difference between OpenRSCAD
and OpenSCAD: one minimal `.scad` repro, one observed OpenSCAD result, one
observed OpenRSCAD result, and one test that will fail until it is fixed.

Atoms sit one level below the two registers we already keep:

| document | grain | answers |
|---|---|---|
| [`COMPAT.md`](../COMPAT.md) | prose entry per behavior family | "what should a user not trust?" |
| [`compatibility/openscad-2021.01.json`](../compatibility/openscad-2021.01.json) | one status per documented surface | "is this surface classified and evidenced?" |
| this document | one measured repro per difference | "what exactly is different, by how much, and what closes it?" |
| [Track F](roadmap/track-f-measured-openscad-compatibility.md) | `F-*` gap per work batch | "what is the plan and priority?" |

A Track F gap usually decomposes into several atoms. `F-G3` alone is three:
`segments` is ignored, implicit `slices` ignores `$fn`, and the profile perimeter
is never refined under twist. Each fails for a different reason and each needs
its own oracle case, so each is tracked separately here.

## How to read an atom

- **id** — `A-<area><nn>`, stable once assigned. Areas mirror Track F: `L`
  language/evaluator, `G` geometry, `I` import, `T` text, `X` export/CLI.
- **class** —
  - **S (silent)** — OpenRSCAD produces a different answer with no diagnostic.
    Release-blocking; a silent wrong answer is a trust bug.
  - **W (warned)** — different, but the user is told at runtime.
  - **P (permanent)** — intentionally different, with a rationale.
  - **M (missing)** — OpenRSCAD refuses or ignores the construct loudly.
  - **U (unproven)** — no known difference, but no upstream oracle evidence
    either. Not a defect; it is the absence of a measurement.
- **delta** — measured, not estimated. Volume/triangle numbers come from the
  protocol below.
- **closes when** — the concrete artifact that retires the atom.

## Measurement protocol

Every number in this document was produced by running both engines on the same
file and comparing exported STL:

```sh
cargo build --release -p openrscad-cli
openscad -o oracle.stl atom.scad          # OpenSCAD 2024.12.17, /opt/homebrew/bin
./target/release/openrscad -o ours.stl atom.scad
```

Volume is the signed-tetrahedron sum over the exported triangles; "empty" means
OpenSCAD reported `Current top level object is empty.` and wrote no file.
Triangle counts are recorded because they expose refinement differences that
volume alone can hide. This is the same geometric comparison the `xtask geom`
oracle uses (volume ±0.1%, bounds/centroid ±0.01 mm), applied by hand to
constructs that have no corpus case yet.

Measurements below were taken at `e011f3f` against OpenSCAD 2024.12.17.

---

## Class S — silent differences

These are the release blockers. Each produces a wrong answer with no warning on
stdout or stderr.

### A-G11 — twist combined with a non-uniform scale refines the profile differently

Twist alone and non-uniform scale alone each have an exact, measured rule (A-G07,
A-G10). Applying both at once follows neither.

| repro | OpenSCAD | OpenRSCAD |
|---|---:|---:|
| `$fa=8;$fs=1.5; linear_extrude(height=7,twist=200,scale=[0.4,1.6],center=true) square([8,5]);` | 246.444 (1244 tris) | 248.329 (956 tris) — **+0.77%** |

class S · gap `F-G3` · manifest `module.linear_extrude.twist_scale_refinement`.
The implementation deliberately keeps the twist-only lengths here rather than
encoding a guess.

**What is now known.** Measuring per-edge counts on `square(10)`, `twist=90`,
`$fa=12`, `$fs=2` over a 5×5 grid of scale factors (`n_x` = segments on each
x-aligned edge, `n_y` on each y-aligned):

| | sy=0.5 | sy=1 | sy=1.5 | sy=2 | sy=3 |
|---|---|---|---|---|---|
| **sx=0.5** | (5,5) | (5,5) | (8,6) | (9,6) | (10,5) |
| **sx=1** | (5,5) | (5,5) | (8,6) | (9,6) | (10,5) |
| **sx=1.5** | (6,8) | (6,8) | (7,7) | (9,6) | (10,5) |
| **sx=2** | (6,9) | (6,9) | (6,9) | (7,7) | (9,6) |
| **sx=3** | (5,10) | (5,10) | (5,10) | (6,9) | (7,7) |

- The table is antisymmetric under swapping the axes, as it must be.
- **Only `max(sx, sy)` matters**, and which axis attains it. Every entry with
  `sy=2, sx≤1.5` is identical, so the rule cannot be a function of the resulting
  edge lengths — `max(original, scaled)` is the same for `sx=1` and `sx=1.5`.
- **The weighting inverts relative to pure scale.** Under `scale=[1,2]` without
  twist the stretching y-edge takes 10 segments and the x-edge 5; add twist and
  it becomes 9 and 6 — the edge that does *not* stretch earns more. Confirmed
  from raw cap coordinates, so it is not an indexing artifact.
- Scaling *down* alone changes nothing: `[0.2,1]` and `[1,0.5]` both give the
  pure-twist `(5,5)`. Only a factor above 1 has any effect.
- Uniform scale ≥1.5 gives `(7,7)` where pure twist gives `(5,5)`, which pins one
  sub-rule exactly: the `$fs` cap uses the edge's **longest length over the
  sweep**, `original × max(1, scale)`. That alone does not explain the
  non-uniform rows, where `n_x` exceeds any such cap.

Sweeping `sx=1` and reading the quotas back out of the tie-blocking (a total of
28 instead of 30 means both fractional parts were exactly ½):

| max scale | 1 | 1.25 | 1.5 | 1.75 | 2 | 2.5 | 3 | 4 | 6 | 10 |
|---|---|---|---|---|---|---|---|---|---|---|
| `(n_x, n_y)` | (5,5) | (7,6) | (8,6) | (9,6) | (9,6) | (10,5) | (10,5) | (10,5) | (10,4) | (11,4) |
| implied `q_x` | 7.5 | — | 8.5 | 9 | 9 | 10 | 10 | 10 | 10.5 | 11 |

`q_x` grows sublinearly in the maximum scale factor and no closed form tried
(linear, `m/(m+1)`, logarithmic, powers) reproduces all of it; the `m=1.25` row
sums to 26, which the tie rule does not explain either, so at least one further
mechanism is involved.

Closes when that mechanism is identified. The per-quad harness that settled
A-G09 is the right tool — applied to segment counts rather than diagonals — and
the base ring must again be taken from the oracle's own output, since OpenSCAD
re-refines even an explicit `polygon()`.

## Class W / P — visible differences

### A-G08 — 3D `minkowski()` of a concave leaf is a convex approximation

| | |
|---|---|
| repro | `minkowski(){ linear_extrude(6) polygon([[0,0],[24,0],[24,6],[6,6],[6,24],[0,24]]); sphere(2,$fn=16); }` |
| OpenSCAD | volume **4312.57**, 412 triangles |
| OpenRSCAD | volume **5739.28**, 510 triangles — **+33.1%** |
| warning | `minkowski: non-convex operand; result is the convex approximation for that part (exact minkowski distributes over union() but not over arbitrary concave meshes)` |
| class | W · gap `F-G7` · manifest `module.minkowski.concave_leaf` |
| closes when | either bounded convex decomposition lands, or Track F records this as permanent with the +33.1% figure |

Unions of convex parts are exact and are not part of this atom
(`corpus/geom/minkowski_union.scad`). The open decision is decomposition versus
permanence — not whether the warning is loud enough.

### A-L02 — `rands()` sequence is not bit-compatible

| | |
|---|---|
| repro | `echo(rands(0,1,3,seed=42));` |
| OpenSCAD | `ECHO: [0.796543, 0.183435, 0.779691]` |
| OpenRSCAD | `ECHO: [0.542627, 0.633134, 0.917741]` |
| class | P · manifest `function.rands` |
| closes when | never — range, reproducibility, and seeded-advance semantics match; the generator deliberately differs |

Kept as an atom so the measured pair is on record: if a future change alters our
sequence, that is a regression against *our* contract even though it can never
match OpenSCAD's.

---

## Class M — missing surface

*Empty.* Every atom that used to sit here is closed: the six retained deprecated
2021.01 forms (`assign`, `child`, `dxf_dim`, `dxf_cross`, `import_stl`,
`import_dxf`), the import selectors and placement, SVG transforms/structure, DXF
curves, AMF/3MF scene assembly, `.csg` export, and text shaping. See the closed
section below for each.

---

## Class U — implemented but unproven

Fifty manifest entries are `implemented`: present, locally tested, no known
difference, but with no committed upstream oracle case. They are not defects and
they are not evidence. Track F's exit criterion requires every 2021.01 core
entry to reach `verified`, so each is an atom-shaped unit of measurement work.

| family | count | examples |
|---|---:|---|
| syntax | 10 | `if/else`, `for`, `let`, blocks, the four modifiers, comments |
| module parameters | 13 | `sphere(d=)`, `cylinder(d1/d2)`, every `convexity=`, `text()` layout |
| export formats | 7 | STL, OFF, AMF, 3MF, DXF, SVG, PNG |
| special variables | 5 | `$t`, `$vpr`, `$vpt`, `$vpd`, `$vpf` |
| modules | 5 | `color`, `render`, `group`, `assert`, `children` |
| functions | 3 | `atan`, `version`, `is_range` (OpenRSCAD extension) |
| file semantics | 3 | raw include/use paths, relative resolution, `OPENSCADPATH` |
| imports | 3 | AMF basic, OBJ, `surface()` PNG heightmaps |
| operators | 1 | vector/matrix arithmetic |

Regenerate the exact list from the manifest:

```sh
python3 -c "
import json
for f in json.load(open('compatibility/openscad-2021.01.json'))['features']:
    if f['status'] == 'implemented':
        print(f['category'], f['id'], '—', f['surface'])
" | sort
```

Promote an entry by adding an `echo:`/`geom:` case and flipping it to
`verified`; `compatibility/validate.py` enforces that the linked case and its
golden both exist.

---

## Closed

### A-I05, A-I07, A-I08, A-I09, A-T01, A-T02 — format and text parity — **closed**

The last of the missing surface, in four pieces. Each is written up in
`COMPAT.md`; what follows is what *measuring* changed about the plan, because in
three of the six the atom's own description turned out to be wrong.

**A-I07/A-I08 — SVG transforms and structure.** The importer scanned tags flat,
so a `<g transform=…>` went unnoticed and `<use>` could not be resolved at all.
It now walks the document with a transform stack. Three rules had to be measured:
`<defs>` draws nothing and an explicit `id=` does *not* override that (though it
does override `display:none`); **`visibility:hidden` does not hide** — upstream
renders it regardless; and with no viewBox coordinates are 1:1 whatever `dpi=`
says. One bug class showed up twice: a self-closing element has no close tag, so
a state entered on it — hidden, or the selection — is never left.

**A-I05 — DXF curves.** `ELLIPSE` was genuinely missing and imported curves were
always cut at the *default* resolution, ignoring the caller's `$fn`. But
**bulges and splines are not supported upstream either**: a two-vertex closed
polyline bulged into a full circle imports as nothing in OpenSCAD 2024.12, and a
`SPLINE` yields no geometry. I had both working before checking, and removed
them — implementing them would have been a divergence, not a fix.

**A-I09 — AMF/3MF scene assembly.** Both readers flattened every vertex in the
file into one index space, so a package of a 2mm and a 3mm cube imported as two
2mm cubes. Fixed. Of the rest of the atom's list, AMF `unit` and
`<constellation>` are *ignored upstream* and we already matched. 3MF
multi-object assembly is now a deliberate divergence in the other direction: see
the entry in `COMPAT.md`.

**A-T01/A-T02 — text shaping.** Runs are shaped with `rustybuzz` rather than
summed glyph by glyph. Arabic went from rendering nothing to exact. The shaper's
*fallbacks* are not upstream's, so the vertical layout had to be derived: each
glyph is centred in a slot of the OS/2 typographic span, which came out of
solving the per-glyph offsets (−1502, −1665, −1298 units for 'a', 'H', 'g' —
ink-centring, not any single metric). Two neighbours fell out of the same work:
`valign` aligns the ink box rather than the ascender, and the default Bézier
flattening was four segments too fine.

### A-X01 — `.csg` tree export was absent — **closed**

| | |
|---|---|
| repro | `openrscad -o out.csg model.scad` |
| OpenSCAD | writes the resolved operation tree |
| before | binary STL named `.csg` (silent), then a format-specific error |
| now | writes the tree; re-rendering it reproduces the geometry |
| was | M · gap `F-I4` · manifest `export.csg`, now `implemented` |
| guarded by | `crates/openrscad-cli/tests/csg_roundtrip.rs` |

`.csg` is the flattened program: modules resolved, expressions evaluated, and
every transform lowered to a `multmatrix` — the format has no
`translate`/`rotate`/`scale`. It needs no geometry, so it is written straight
from the tree before the render pass, and `$preview` stays true for it exactly
as upstream reports.

**The contract is a round trip, not byte-identical text**, and that is a
deliberate call. OpenSCAD omits parameters left at their defaults; the IR does
not record whether a value was written or defaulted, so reproducing that
omission is not possible from what we keep. Writing every parameter explicitly
is equally valid input, and the meaningful question — does re-rendering give the
same solid — is testable. Verified two ways: 19 constructs re-rendered through
OpenSCAD itself matched the original, and the in-repo test re-renders with our
own engine and additionally asserts a second export is a fixed point, which
catches a writer and reader that disagree about something the first pass
happened to survive.

Two details worth recording:

- **A round trip is lossy by construction.** The format writes six significant
  digits, so a rotation matrix returns slightly rounded — a 6.0 volume comes
  back 5.999994. Upstream's own round trip loses exactly the same precision, so
  the test tolerance reflects the text format rather than pretending otherwise.
- **An omitted `slices=` must stay omitted.** Writing the resolved count would
  freeze the tessellation that this render happened to choose; the reader has to
  derive it again from the profile.

### A-I03, A-I04, A-I06 — import selectors were accepted and ignored — **closed**

| id | repro | OpenSCAD | before | now |
|---|---|---:|---:|---|
| A-I03 | `import("layers.dxf", layer="A")` | 16 | 20 (whole file) | 16 |
| A-I04 | `…, origin=[1,1], scale=2` | 64 | 20 (untransformed) | 64 |
| A-I06 | `import("layers.svg", layer="LayerA")` | 1.991 | 3.609 (whole file) | 1.991 |

was M (arguably S — the import succeeded and returned the wrong contents) ·
gap `F-I2` · manifest `import.dxf.layer`, `import.dxf.origin_scale`,
`import.svg.layer_id`, all now `verified`. Guarded by
`corpus/geom/import_{dxf,svg}_selectors.scad` with committed fixtures, `tris`
pinned.

Placement is `(point - origin) * scale`, confirmed by bounding box and not just
area: `origin=[1,1], scale=2` moves a 0…4 square to −2…6. Both are 2D-only, as
upstream — a mesh import ignores them.

SVG selection pulls the matching subtree out of the source text before the
element walk, rather than teaching that walk to track nesting. The walk is flat,
and making it hierarchical is the separate A-I08; extracting the subtree first
gets `layer=`/`id=` exactly right without waiting on it.

### A-L03…A-L06, A-I01, A-I02 — the deprecated 2021.01 forms — **closed**

All six of gap `F-L6`, implemented rather than declared out of scope: they are
part of the 2021.01 baseline OpenSCAD still accepts.

| id | surface | now |
|---|---|---|
| A-L03 | `assign(bindings) body` | scoped bindings + deprecation notice |
| A-L04 | `child(index)` | singular child selection + notice |
| A-L05 | `dxf_dim(file, layer, origin, scale, name)` | reads a DIMENSION measurement |
| A-L06 | `dxf_cross(file, layer, origin, scale)` | reads a line intersection |
| A-I01 | `import_stl(...)` | alias for `import()` + notice |
| A-I02 | `import_dxf(...)` | alias for `import()` + notice |

Guarded by `corpus/echo/deprecated_forms.scad`, `corpus/echo/dxf_query.scad`
(with committed DXF fixtures), `corpus/geom/deprecated_assign_child.scad`, and
`corpus/geom/import_alias_{stl,dxf}.scad`. 23 oracle-compared cases for the DXF
readers alone.

Four details the oracle settled that the names actively mislead about:

- **`assign()` is not `let()`.** Every right-hand side evaluates in the
  *enclosing* scope and the bindings take effect together: with `x = 100`,
  `assign(x = 1, y = x + 1)` yields `y == 101`, where `let` gives 2.
- **Bare `child()` is not bare `children()`.** It means the first child alone;
  `children()` means all of them.
- **`dxf_dim`'s positional order is `(file, layer, origin, scale, name)`** —
  `name` comes *last*. Passing it third silently binds it to `origin`, which is
  how upstream behaves and what the oracle showed when the third positional
  produced "origin could not be converted".
- **`dxf_dim` matches on the dimension *text* (group 1), not the block name, and
  ignores the stored measurement (group 42)**, recomputing from the definition
  points: a fixture whose 42 says 99 still reports the geometric 5. Linear
  dimensions project onto the group-50 rotation (a 6×8 offset reads 6 at 0°, 8
  at 90°, 9.899 at 45°), aligned ones use the plain distance, and radius and
  diameter both report the centre-to-chord distance.

### A-G09 — non-planar wall quads used the wrong diagonal — **closed**

| repro | OpenSCAD | before | now |
|---|---:|---:|---|
| twisted square with a hole | 847.626 | 842.424 (+0.61%) | 847.626 |
| twisted square, two holes | 3762.287 | 3753.624 (+0.23%) | 3762.287 |
| twisted square off the axis | 987.383 | 1000.14 (+1.29%) | 987.383 |
| …with `segments=8` | 1006.52 | 1038.41 (+3.17%) | 1006.52 |
| `$fn=24` circle, `scale=[0.2,2]` | 646.225 | 647.048 (+0.13%) | 646.225 |

was S · gap `F-G3` · manifest `module.linear_extrude.twist_diagonal`, now
`verified`. Guarded by `corpus/geom/ext_linear_twist_hole.scad`,
`ext_linear_twist_offaxis.scad` and `ext_linear_scale_round.scad`, all with
`tris` pinned.

**The rule: split each quad along its shorter diagonal; when the two are exactly
equal, fall back to the wall's lean** (the twist direction, flipped for a hole
because its indices run the other way round).

This atom resisted two earlier attempts, and the reason is worth recording,
because the failure was one of method rather than of the hypothesis. Both
attempts guessed a *global* rule and scored it by comparing final volumes — and
"shorter diagonal" was tried and rejected that way, because it made things
worse. It was the right rule all along: ties are pervasive (any profile
symmetric about the sweep leaves the two diagonals exactly equal — the plain
twisted square is 160 quads, *all* ties), and resolving them arbitrarily
corrupted the majority of quads while the minority that actually differ were
being fixed.

What broke it open was measuring the *dependent variable directly* instead of a
downstream aggregate: reconstructing the ring/index structure and reading which
diagonal OpenSCAD used **per quad**, then scoring candidate predicates against
3142 individual choices spanning positive and negative twist, off-axis profiles,
pure scale, and a 720° sweep. With that signal the pattern was immediate — the
choice is identical across layers and varies only by profile edge — and the
composite rule scored 3142/3142 on the first try.

Two details the per-quad data settled that volume comparisons never could:

- the base ring must be taken from the *oracle's own output*, not assumed:
  OpenSCAD re-refines even an explicit `polygon()`, so a hand-built profile
  silently desynchronises the index mapping and every "measurement" after that
  is noise;
- the tie-break needs the winding term, so a hole leans opposite its outer.

### A-G10 — non-uniform `scale` did not refine the profile — **closed**

| repro | OpenSCAD | before | now |
|---|---|---|---|
| `linear_extrude(height=10, scale=[0.2,2]) square(10);` | 596 tris (30 segments, 9 slices) | 12 tris (4 segments, 1 slice) | 596 tris |
| `linear_extrude(height=10, scale=[2,1]) square([10,3]);` | 428 tris | 12 tris | 428 tris |
| `linear_extrude(height=10, scale=0.5) square(10);` (uniform) | 12 tris | 12 tris | 12 tris |

was S · gap `F-G3` · manifest `module.linear_extrude.scale_refinement`, now
`verified`. Guarded by `corpus/geom/ext_linear_scale_nonuniform.scad` and
`ext_linear_scale_uniform_tris.scad`.

A non-uniform scale bends the walls exactly as a twist does, and upstream
refines for it; a *uniform* scale keeps every wall planar — a frustum's faces
are flat — so it refines nothing however far from 1 it is. The rules, measured
the same way as the twist family:

- **Refinement** reuses the twist machinery, but an edge earns its share of the
  budget by `max(original, scaled)` length: under `scale=[1,2]` the y-aligned
  edges stretch to 20 and take twice the share of the x-aligned ones. The `$fs`
  cap uses the same stretched length, which is why raising `$fs` from 2 to 1
  changes nothing on a square — the budget, not `$fs`, is binding.
- **Slices** come from how far the worst-placed profile point travels to its
  scaled position: `ceil(hypot(travel, height) / $fs)`, matched at four heights
  and `$fs` values. `$fa` plays no part — there is no angle to bound — and `$fn`
  replaces the count outright. Twist and scale each propose a count and the
  larger wins.

**This is volume-neutral**, because the walls stay ruled surfaces: the scaled
square is 833.333 either way. Only the tessellation changes, so both corpus
cases pin `tris` — the first use of the `// oracle: tris` directive. Blessing
them turned up a wrinkle worth recording: a case must hold **one** top-level
object, because several are unioned and the kernel merges the coplanar wall
facets straight back together, collapsing 596 triangles to 12 and hiding the
very thing the case exists to check.

What did *not* close: a non-uniformly scaled **curved** profile is still 0.13%
off. Its cap points, slices and triangle count all match, so that residue is the
wall-diagonal rule and now sits under A-G09.

### A-X01, A-X02 — unusable export suffixes silently wrote binary STL — **closed**

| id | repro | OpenSCAD | before | now |
|---|---|---|---|---|
| A-X02 | `openrscad -o out.foo m.scad` | `Invalid suffix foo`, exit 1, no file | binary STL, exit 0 | error, exit 1, no file |
| A-X01 | `openrscad -o out.csg m.scad` | writes a CSG tree, exit 0 | binary STL named `.csg`, exit 0 | error, exit 1, no file |

was S · gap `F-X1` · manifest `export.suffix_validation` (now `implemented`) and
`export.csg`. Guarded by unit tests in `crates/openrscad-cli/src/main.rs`.

Suffix classification is now one function, checked against the 2024.12 binary
across eleven suffixes: recognized ones match case-insensitively (`out.STL`),
and anything else — including no suffix at all — is refused. Validation runs
*before* evaluation, so a typo on a heavy model fails in milliseconds instead of
after the render.

**A-X02 is fully closed; A-X01 is downgraded, not closed.** We still cannot
serialize a CSG operation tree, so `.csg` now fails with a format-specific error
rather than a generic "invalid suffix" — it is a real OpenSCAD format, not a
typo. That converts A-X01 from a *silent* wrong answer into a *loud* missing
feature, which is the property that matters for trust; the feature itself
remains open as `F-I4`.

The manifest keeps `export.suffix_validation` at `implemented` rather than
`verified`, matching every other `export.*` entry: `verified` there means a
committed `.scad`-level oracle case, and a CLI argument behaviour has none. The
atom is closed because it is fixed and guarded by a test — the two documents
mean different things by their labels, deliberately.

### A-G01…A-G04 — invalid dimensions built solids instead of nothing — **closed**

| id | repro | OpenSCAD | before | now |
|---|---|---|---:|---|
| A-G01 | `cube([-2,3,4]);` | empty | volume 24 | empty |
| A-G02 | `cylinder(h=-5, r=2);` | empty | volume 54.7282 | empty |
| A-G03 | `linear_extrude(1) square([-2,3]);` | empty | volume 6 | empty |
| A-G04 | `linear_extrude(height=-5) square(2);` | empty | volume 20 | empty |

was S · gap `F-G2` · manifest `module.{cube,cylinder,square}.invalid_dimensions`
and `module.linear_extrude.invalid_height`, all now `verified`; the sweep also
covered `sphere(r<=0)` and `circle(r<=0)`, which gained entries. Guarded by
`corpus/geom/prim_invalid_dims.scad`.

Measuring this turned up something the register understated. The zero cases —
`cube(0)`, `sphere(0)`, `cylinder(h=0)` — did not merely produce a zero-volume
solid; they emitted **non-manifold triangles that broke the enclosing boolean**:

```scad
difference() { cube(10); cube(0); }
// before: WARNING: difference: kernel error: ManifoldStatus(NotManifold)
//         -- showing un-combined geometry (the boolean was skipped)
```

So a stray `cube(0)` anywhere in a model could silently disable a CSG operation
elsewhere in the tree. That is a strictly larger blast radius than "produces a
degenerate solid", and it is why this was worth closing before the remaining
sub-percent geometry atoms.

The rule is uniform — every dimension must be **finite** and strictly positive —
with two exceptions worth stating, both of which the oracle had to settle:

- `cylinder(r1=0, r2=3)` is a legitimate cone and stays; both radii zero is empty.
- For `linear_extrude` a *non-finite* height (`inf`, `NaN`) counts as **unset**
  and falls back to the default 100, where for a primitive it means empty.
  `linear_extrude(height=1/0) square(2)` is upstream a solid of volume 400.

`linear_extrude(height=0)` still evaluates its children, so their `echo`/`assert`
side effects run. NaN and infinity were the parts most likely to be got wrong by
inspection: `x <= 0.0` is false for NaN, so a "reject non-positive" test written
the obvious way would have built geometry from `cube(0/0)`.

### A-L08 — `$` arguments did not reach children — **closed**

| | |
|---|---|
| repro | `linear_extrude(height=1, $fn=32) circle(5);` |
| OpenSCAD | the circle sees `$fn=32` → 32-gon, area **78.036** |
| OpenRSCAD (before) | fell back to `$fa`/`$fs` → 16-gon, area **76.537** (−1.9%) |
| OpenRSCAD (now) | **78.036** |
| was | S · gap `F-L7` · manifest `syntax.arguments.special_variable_scope`, now `verified` |
| guarded by | `corpus/echo/special_args.scad`, `corpus/geom/special_args_fn.scad` |

A `$` argument is dynamically scoped over the callee *and* everything under it.
It was reaching user module bodies but not builtin modules' children, and
function calls dropped it entirely — it is not a declared parameter, so the
binding map discarded it before the body ran.

The subtle part is that this must not cost an extra evaluation. Upstream
evaluates every argument exactly once, in the caller's scope, so all of these
had to keep holding:

| repro | expected | why it constrains the fix |
|---|---|---|
| `m($fa=$fa/2)` with `$fa=12` | 6 inside, 12 after | publishing before the rest are evaluated would compound to 3 |
| `m($fn=7, $fa=$fn)` with `$fn=0` | `7, 0` | the second argument must not see the first |
| `m($fn=echo("x") 8)` | `"x"` once | a naive push-then-bind evaluates the expression twice |

So the values are computed once up front in the caller's scope, then published
into a dynamic frame that wraps the call; the binders read them back instead of
re-evaluating. `translate()` and friends never look at named arguments at all,
which is why filling the frame from the argument binder alone was not enough.

This retired two BOSL2 expected failures — both `test_segs`, which exercises
exactly this idiom — taking that gate from 503/513 to **505/513** and shrinking
the expected-failure list from ten to eight.

### A-G05, A-G06, A-G07 — `linear_extrude` refinement under twist — **closed**

Three atoms in one code path, all silent volume errors, all now exact against
OpenSCAD 2024.12.17:

| atom | was | now |
|---|---|---|
| A-G05 `segments=` accepted and ignored | +6.1% | exact |
| A-G06 omitted `slices` ignores `$fn`/`$fa`/`$fs` | +7.4% | exact |
| A-G07 profile never re-tessellated under twist | +16.4% | exact |

Measured across an 18-case matrix, cases outside the 0.1% oracle tolerance went
from **15/18 to 2/18**; the two that remain (A-G09, A-G10) improved from 6.5% and
7.9% to 0.61% and 0.77%.

| repro | oracle | before | after |
|---|---:|---:|---:|
| `twist=90` defaults | 1006.601 | 1074.915 (+6.8%) | 1006.601 |
| `twist=90, $fn=40` | 1001.111 | 1074.915 (+7.4%) | 1001.111 |
| `twist=90, slices=3, $fa=3, $fs=0.5` | 963.675 | 1122.009 (+16.4%) | 963.675 |
| `twist=180, $fa=6, $fs=1` on `square([10,3])` | 301.717 | 343.611 (+13.9%) | 301.717 |
| `twist=-90` | 1006.601 | 902.369 (−10.4%) | 1006.601 |
| `twist=720, $fa=6, $fs=1` | 243.133 | 271.783 (+11.8%) | 243.133 |

Guarded by `corpus/geom/ext_linear_twist{,_fn,_profile,_neg,_round,_shape}.scad`
and `ext_linear_segments{,_twist}.scad`, plus unit tests in
`crates/openrscad-geom/src/shape2d.rs` that pin the derived rules without needing
the oracle binary.

The three rules, each derived black-box by exporting meshes and counting the
points on every profile edge (no OpenSCAD source was read), then validated at
130/130 randomized cases before any code was written:

- **Profile refinement.** Each closed contour gets a budget — `segments=` if
  given, else `$fn`, else `360/$fa` — apportioned across its edges in proportion
  to length. An edge whose share is under one segment takes one anyway and
  leaves the pool, shrinking the budget for the rest. Whole segments go out
  first, the remainder to the largest fractional shares; a tie wider than what
  is left goes unawarded, which is why an equilateral outline rounds *down*
  (a square at `$fa=12` gets 28 segments, not 30). `$fs` then caps each edge at
  `ceil(len/$fs)`, but only when `$fa` set the budget. Without twist there is no
  refinement unless `segments=` asks for it.
- **Slice count.** The tighter of two limits: no slice twists more than `$fa`
  degrees, and no slice moves the outermost profile point more than `$fs` along
  its helical path, `hypot(r·twist, height)`. `$fn` replaces both with `$fn`
  slices per revolution.
- **Wall diagonal.** A twisted quad is non-planar, so its two diagonals enclose
  different volumes — on a 32-gon twisted one vertex step per slice, one gives
  the prism exactly and the other cuts 1.3% off. The split follows the twist
  direction and the contour winding. (Still wrong for off-axis and holed
  profiles: see A-G09.)

### A-L01 — `$preview` was `true` during exact render and export — **closed**

| | |
|---|---|
| repro | `if ($preview) sphere(20); else cube(10);` |
| OpenSCAD | takes the `else` branch during `-o` export → volume **1000**, 12 triangles |
| OpenRSCAD (before) | took the `if` branch → volume **32902.9**, 896 triangles |
| OpenRSCAD (now) | volume **1000**, 12 triangles |
| was | S · gap `F-L2` · manifest `special.preview`, now `verified` |
| guarded by | `corpus/geom/preview_branch.scad` (exact side, binary-STL oracle) and `corpus/echo/preview_mode.scad` (preview side, echo oracle) |

Evaluation mode became an explicit `RenderMode` input instead of a hardcoded
`true`. The measured upstream rule — `$preview` is false *exactly* when an exact
render happens — now holds on every path:

| invocation | OpenSCAD | OpenRSCAD |
|---|---|---|
| `-o out.stl` (and other mesh formats) | false | false |
| `-o out.dxf` / `-o out.svg` | false | false |
| no output (stats) | n/a | false |
| `-o out.png` | true | true |
| `--render -o out.png` | false | false |
| `--export-format=echo` / `--check` | true | true |
| `-D '$preview=true'` | true | true |

Both gates were needed: pinning only the export side would let a regression flip
echo-only runs to `false`, and pinning only echo would not have caught the
original bug at all. The two corpus cases disagree with each other by design —
that is what makes the mode observable.

Hosts now choose explicitly rather than inherit a default: wasm and desktop
derive the mode from the fast-preview flag they already thread, the LSP uses
preview for analysis and live rendering but exact for its `openrscad.render`
export command, and `xtask echo`/`xtask geom` match the mode of the oracle
invocation each one compares against.

## Ledger

| class | atoms | meaning |
|---|---:|---|
| S — silent | 1 | A-G11 |
| W — warned | 1 | A-G08 |
| P — permanent | 1 | A-L02 |
| M — missing | 0 | — |
| U — unproven | 50 | manifest `implemented` entries |
| closed | 28 | A-L01, A-G01…A-G04, A-G05…A-G07, A-G09, A-G10, A-L03…A-L06, A-L08, A-I01…A-I09, A-T01, A-T02, A-X01, A-X02 |

**Class M is empty**: every missing surface is closed. One silent atom remains
(A-G11), one warned divergence, and one permanent one; the 50 `unproven` entries
are measurement work, not defects.

Six of the atoms were found by measuring rather than by reading docs:
A-X01/A-X02 while writing this register, and A-G09/A-G10/A-L08 while closing the
twist family. Fixing one atom exactly is what exposed the next three — each was
hidden behind an error an order of magnitude larger.

Measuring also *removed* work three times. The register listed DXF bulges and
splines, AMF units, and AMF constellations as gaps; the oracle shows OpenSCAD
supports none of them, so implementing any would have been a new divergence
rather than a fix. An atom is only worth closing once you have checked which
side of it upstream is on.

## Maintaining this document

1. **One atom, one repro, one number.** If a repro needs two sentences of "and
   also", it is two atoms. Splitting is cheap; a half-closed atom is not.
2. **Measure both engines before adding a row.** Use the protocol above and
   record the version and commit. An atom without numbers is a TODO, not an atom.
3. **Keep the links live.** Every atom names its Track F gap and its manifest
   entry, or explicitly says none exists yet. When the manifest gains an entry,
   update the row.
4. **Retire, don't delete.** A closed atom moves to a "Closed" section with the
   corpus case that now guards it, so a regression has somewhere to point.
5. **Class S is a release blocker.** Anything that lands here must also appear in
   `COMPAT.md` under "Known silent differences" until it is fixed.
