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

### A-G01…A-G04 — invalid dimensions build solids instead of nothing

OpenSCAD treats a non-positive dimension as "no geometry" and writes no file.
OpenRSCAD builds a reflected solid and exports it.

| id | repro | OpenSCAD | OpenRSCAD |
|---|---|---:|---:|
| A-G01 | `cube([-2,3,4]);` | empty | volume 24 |
| A-G02 | `cylinder(h=-5, r=2);` | empty | volume 54.7282 |
| A-G03 | `linear_extrude(1) square([-2,3]);` | empty | volume 6 |
| A-G04 | `linear_extrude(height=-5) square(2);` | empty | volume 20 |

class S · gap `F-G2` · manifest `module.cube.invalid_dimensions`,
`module.cylinder.invalid_dimensions`, `module.square.invalid_dimensions`,
`module.linear_extrude.invalid_height`.

Closes when a shared validation path yields empty geometry (plus OpenSCAD's
warning) for every non-positive dimension, with one corpus case per primitive.
Track these as four atoms rather than one: they live in different constructors
and a single fix is easy to land for three of them and miss the fourth.

### A-G09 — twisted extrudes of off-axis or holed profiles pick the wrong wall diagonal

A twisted wall quad is not planar, so its two diagonals enclose different
volumes. The split now follows the twist direction and the contour winding,
which is exact for profiles that straddle the Z axis — but not yet for a profile
translated away from it, or for a hole.

| repro | OpenSCAD | OpenRSCAD | delta |
|---|---:|---:|---:|
| `linear_extrude(height=10,twist=90) difference(){square(10);translate([3,3])square(4);}` | 847.626 | 842.424 | +0.61% |
| `linear_extrude(height=10,twist=90,slices=4) translate([20,0]) square(10);` | 987.383 | 1000.14 | +1.29% |
| `linear_extrude(height=10,twist=90,slices=4,segments=8) translate([20,0]) square(10);` | 1006.52 | 1038.41 | +3.17% |

The vertex sets are *identical* to OpenSCAD's in these cases — only the
triangulation differs — so the segment and slice counts are right and this is
purely the diagonal rule. No single global rule fits: forcing either diagonal,
or choosing the shorter one per quad, is worse overall, and on the holed case
OpenSCAD disagrees with our choice on only half the hole's quads. It evidently
uses a local criterion that has not been identified.

class S · gap `F-G3` · manifest `module.linear_extrude.twist_diagonal`. Closes
when the per-quad criterion is identified, gated by holed and off-axis twisted
corpus cases.

### A-G10 — non-uniform `scale` does not refine the profile or add slices

Non-uniform scaling makes the walls non-planar in the same way twist does, and
OpenSCAD refines for it. OpenRSCAD only refines for twist.

| repro | OpenSCAD | OpenRSCAD |
|---|---|---|
| `linear_extrude(height=10, scale=[0.2,2]) square(10);` | 596 tris (30 profile segments, 9 slices) | 12 tris (4 segments, 1 slice) |
| `linear_extrude(height=7,twist=200,scale=[0.4,1.6],center=true,$fa=8,$fs=1.5) square([8,5]);` | 246.444 (1244 tris) | 248.329 (956 tris) — **+0.77%** |

Scale alone is volume-neutral (the walls stay ruled surfaces, so 833.333 either
way) and only the triangle count differs; combined with twist it moves the
volume. Uniform scale correctly refines nothing.

Partially characterised: the slice count is
`ceil(hypot(max travel of a profile point, height) / $fs)`, verified at four
heights and `$fs` values. The per-edge refinement uses `max(original, scaled)`
edge length rather than the twist rule's budget apportionment, and the combined
twist+scale case does not yet fit either. class S · gap `F-G3` · manifest
`module.linear_extrude.scale_refinement`.

### A-L08 — `$fn`/`$fa`/`$fs` passed as call arguments do not reach children

| | |
|---|---|
| repro | `linear_extrude(height=1, $fn=32) circle(5);` |
| OpenSCAD | the circle sees `$fn=32` → 32-gon, area **78.036** |
| OpenRSCAD | the circle falls back to `$fa`/`$fs` → 16-gon, area **76.537** (−1.9%) |
| class | S · gap `F-L7` · manifest `syntax.arguments.special_variable_scope` |
| closes when | a `$`-prefixed argument to a module call is pushed onto the dynamic frame for that call's children |

`$fn=32; linear_extrude(height=1) circle(5);` (outer variable) and
`circle(5,$fn=32)` (on the child itself) are both correct, so this is specific to
the argument form. OpenSCAD scopes a `$` argument dynamically over the callee's
children; we bind it only for the call itself. Found while measuring A-G07 —
it is an evaluator scoping bug, not a geometry one, and it silently changes the
resolution of every child of such a call.

### A-X01 — `-o out.csg` silently writes binary STL

| | |
|---|---|
| repro | `openrscad -o out.csg model.scad` where `model.scad` is `cube(3);` |
| OpenSCAD | writes the CSG tree: `cube(size = [3, 3, 3], center = false);` |
| OpenRSCAD | writes 684 bytes of **binary STL** named `out.csg`, prints `wrote out.csg`, exits 0 |
| class | S · gaps `F-I4`, `F-X1` · manifest `export.csg` |
| closes when | `.csg` either serializes the operation tree or fails with an unsupported-format error; CLI fixture asserts the file is not STL |

The manifest classifies `export.csg` as `missing`, which understates it: the
format is not merely absent, it is silently substituted. A user asking for a CSG
tree gets a mesh with the wrong extension.

### A-X02 — unknown export suffixes fall back to binary STL

| | |
|---|---|
| repro | `openrscad -o out.foo model.scad` |
| OpenSCAD | `Invalid suffix foo. Either add a valid suffix or specify one using the --export-format option.` |
| OpenRSCAD | writes binary STL to `out.foo`, exits 0 |
| class | S · gap `F-X1` · manifest `export.suffix_validation` |
| closes when | unrecognized suffixes are rejected, or an explicit format flag is required; CLI test asserts a non-zero exit |

A-X01 is a consequence of A-X02, but they close differently: A-X02 is CLI
argument handling, A-X01 additionally needs the tree serializer. Keep both.

---

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

Loud failures. No silent wrong answers here, but each is a script that runs
upstream and does not run here.

| id | repro | OpenSCAD | OpenRSCAD | manifest |
|---|---|---|---|---|
| A-L03 | `assign(x=5) echo(x);` | `DEPRECATED: …assign() will be removed…` then `ECHO: 5` | `WARNING: Ignoring unknown module 'assign'`, no echo | `syntax.assign_legacy` |
| A-L04 | `module m(){ child(0); } m() cube(1);` | `DEPRECATED: child()…`, renders the cube | `WARNING: Ignoring unknown module 'child'`, empty | `module.child_legacy` |
| A-L05 | `echo(dxf_dim(file="x.dxf", name="d"));` | opens the file (warns if absent), `ECHO: undef` | `ECHO: undef` + unknown-function warning | `function.dxf_dim` |
| A-L06 | `echo(dxf_cross(file="x.dxf", layer="l"));` | as above | as above | `function.dxf_cross` |
| A-I01 | `import_stl("m.stl");` | `DEPRECATED: …`, imports | `WARNING: Ignoring unknown module 'import_stl'` | `import.alias_stl` |
| A-I02 | `import_dxf("m.dxf");` | `DEPRECATED: …`, imports | `WARNING: Ignoring unknown module 'import_dxf'` | `import.alias_dxf` |

All six are gap `F-L6` and share one decision: implement the retained deprecated
2021.01 aliases, or declare them out of scope. A-L05/A-L06 are the mildest — the
returned value already matches; only the diagnostic and the file access differ.

### Import and text atoms

These are known-incomplete rather than individually measured; each needs a
fixture before it becomes a proper atom with numbers.

| id | surface | gap | manifest |
|---|---|---|---|
| A-I03 | `import(…, layer=)` for DXF — accepted, ignored | `F-I2` | `import.dxf.layer` |
| A-I04 | `import(…, origin=, scale=)` for DXF — accepted, ignored | `F-I2` | `import.dxf.origin_scale` |
| A-I05 | DXF bulges, splines, ellipses, caller fragment controls | `F-I2` | `import.dxf.curves` |
| A-I06 | SVG `layer`/`id` selectors — accepted, ignored | `F-I2` | `import.svg.layer_id` |
| A-I07 | SVG transforms, units, DPI | `F-I2` | `import.svg.transforms_dpi` |
| A-I08 | SVG nesting, `<use>`, style, visibility | `F-I2` | `import.svg.structure_style` |
| A-I09 | AMF/3MF units, object index spaces, components, build transforms | `F-I3` | `import.amf_3mf.scene_graph` |
| A-T01 | `text()` kerning, ligatures, complex-script shaping | `F-I1` | `module.text.shaping` |
| A-T02 | `text(direction=, language=, script=)` — RTL only reverses codepoints | `F-I1` | `module.text.direction_language_script` |

An accepted-and-ignored parameter (A-I03, A-I04, A-I06) is arguably class S, not
M: the import succeeds and returns the wrong contents. They are listed here
because that is how the manifest currently classifies them; re-classifying is a
decision for the `F-I2` batch, not a documentation change.

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
| S — silent | 9 | A-G01…A-G04, A-G09, A-G10, A-L08, A-X01, A-X02 |
| W — warned | 1 | A-G08 |
| P — permanent | 1 | A-L02 |
| M — missing | 15 | A-L03…A-L06, A-I01…A-I09, A-T01, A-T02 |
| U — unproven | 50 | manifest `implemented` entries |
| closed | 4 | A-L01, A-G05, A-G06, A-G07 |

Six of the open atoms were found by measuring rather than by reading docs:
A-X01/A-X02 while writing this register, and A-G09/A-G10/A-L08 while closing the
twist family. Fixing one atom exactly is what exposed the next three — each was
hidden behind an error an order of magnitude larger.

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
