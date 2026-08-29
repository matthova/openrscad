# Compatibility divergence register

The release-blocking baseline is the stable language and modeling surface of
OpenSCAD 2021.01. Later stable features are tracked separately; experimental
features and GUI pixel parity are not implied. Mesh compatibility is measured by
geometry (volume, bounds, centroid, topology), not identical bytes or triangle
ordering. The full closure plan and scope are in
[Track F](docs/roadmap/track-f-measured-openscad-compatibility.md), with the
classified surface in the [compatibility manifest](compatibility/README.md).

This register records confirmed differences. A silently wrong answer is a trust
bug, so silent entries stay here until fixed; intentional differences must warn
at runtime or be justified as permanent.

Each entry below is decomposed into minimal, individually measured repros in the
[compatibility atom register](docs/compat-atoms.md) — one atom per closable
difference, with the observed OpenSCAD and OpenRSCAD numbers for each.

## Known silent differences

- **`textmetrics()` reports the ink box from exact glyph extents, not OpenSCAD's
  FreeType-scaled outline.** OpenSCAD loads each glyph through FreeType at a tiny
  pixel em (`size·100/72`) and reads the metrics off the 26.6 fixed-point
  outline, so its ink extremes are pushed out by the quantization. We compute the
  box from the font's exact glyph bounding boxes instead, which is deterministic,
  `$fn`-independent, and consistent with the geometry `text()` actually emits. The
  `position`/`size`/`ascent`/`descent` fields therefore differ by up to ~0.01mm;
  `advance`, `offset`, and every alignment relationship are exact. `fontmetrics()`
  is a straight scaling of the face's own tables and matches the oracle to <1e-3.
  Both are experimental in OpenSCAD (they require `--enable=textmetrics`).

  ```scad
  echo(textmetrics("Hello", size=10).descent); // -0.135634 here, -0.1408 upstream
  ```

## Missing or partial compatibility

- **BOSL2 function-suite coverage is partial and gated.** `xtask bosl2` passes
  511/513 pinned blocks across 15 files. The two expected failures are
  `test_gaussian_rands` (needs OpenSCAD's exact PRNG, an intentional divergence
  shared with `rands()`) and `test_f_acos` (asserts trig results *exactly* —
  `acos(0.5) == 60` — where our `x.acos().to_degrees()` carries a ~1e-15 error
  and OpenSCAD returns the exact value). Measurement shows this is **not** a
  digit-rounding step: OpenSCAD keeps full libm precision for most angles
  (`cos(1) == 0.9998476951563913`) yet nails the special ones (`cos(60) == 0.5`,
  `cos(45)` raw but `cos(30)` cleaned), so matching it means reproducing
  OpenSCAD's specific floating-point **argument-reduction** algorithm across
  every trig entry point — a broader numerical-parity change, left to its own
  pass. This is broad library evidence, not the complete BOSL2 module suite.

## Warned divergences

- **`roof()` is exact for a convex profile; a concave profile or one with holes
  is warned and skipped.** A convex outline's straight skeleton needs only *edge
  events* (an edge marches inward until it vanishes), which is simulated exactly:
  squares, rectangles, triangles, and regular polygons match the oracle in
  volume, bounds, and centroid (`corpus/geom/roof_*.scad`), and `roof()` is a
  manifold solid usable in booleans. A concave outline introduces *split events*
  (a reflex vertex slicing the wavefront), and holes an inner wavefront; both
  need the general straight-skeleton algorithm and are out of scope, so they warn
  and produce nothing rather than a wrong roof. `roof()` is experimental upstream
  (`--enable=roof`); OpenRSCAD enables it unconditionally.

  ```scad
  roof() square(10);                 // exact: a pyramid, apex (5,5,5)
  roof() circle(6, $fn=6);           // exact: hexagon to a central apex
  roof() polygon([[0,0],[20,0],[20,8],[8,8],[8,20],[0,20]]);  // warned + skipped
  ```

- **3D `minkowski()` is exact for convex operands and unions of them; a concave
  *leaf* mesh is a convex approximation (warned).** The convex-convex sum is the
  convex hull of pairwise vertex sums. Minkowski distributes over `union()`
  (`(A₁∪A₂) ⊕ B = (A₁⊕B) ∪ (A₂⊕B)`), so a non-convex shape *built from a union of
  convex parts* — the common way to build concave shapes — is now **exact**
  (`corpus/geom/minkowski_union.scad`). A genuinely concave *leaf* (e.g. a
  concave polygon extruded, or a non-convex polyhedron) still falls back to its
  convex hull with a warning — exact convex decomposition of arbitrary meshes is
  out of scope (it is research-grade and, as in OpenSCAD's CGAL, impractically
  slow). **2D `minkowski()` is always exact** (each operand is triangulated and
  the pairwise sums are unioned).

  ```scad
  // exact: concave shape as a union of convex parts
  minkowski() { union() { cube([10,4,4]); cube([4,10,4]); } cube(2, center=true); }
  // approximated + warned: a concave leaf that can't be peeled into a union
  minkowski() { linear_extrude(6) polygon([[0,0],[24,0],[24,6],[6,6],[6,24],[0,24]]); sphere(2); }
  ```

## Permanent divergences

- **`version()` reports the baseline OpenRSCAD targets, not an upstream build.**
  `version()` is `[2021, 1, 0]` and `version_num()` is `20210100`, where the
  oracle reports whichever OpenSCAD is installed (2024.12.17 → `20241217`). An
  engine reports its own version; impersonating a specific upstream build would
  be the actual bug. The consequence worth knowing is that a script gating on
  `version_num()` may take a different branch here.

  ```scad
  echo(version(), version_num());   // [2021, 1, 0], 20210100
  ```

- **A multi-object 3MF imports every object; OpenSCAD imports one.** Upstream
  2024.12 returns only the highest-`id` object in `<resources>` and ignores
  `<build>` entirely — it will even import an object the build never references,
  and drops the rest. Measured, not inferred: a package of 2mm, 3mm and 4mm
  cubes imports as the 4mm cube alone, whichever order they are declared in.
  OpenRSCAD assembles the build instead, honouring per-item transforms and
  repeat instancing, because silently discarding a user's geometry is the worse
  failure — and our own colour-group 3MF export writes one object per colour, so
  matching upstream would make that export unreadable by us too. Single-object
  packages, which is nearly everything in the wild, are identical either way.

  ```scad
  import("two-objects.3mf");   // both objects here; the larger-id one upstream
  ```

- **`rands()` is not bit-compatible.** OpenRSCAD uses an xorshift PRNG; values
  are reproducible and global/seeded advance semantics match, but the sequence
  differs from OpenSCAD's generator. This is intentional.

  ```scad
  echo(rands(0, 1, 3, seed=42)); // reproducible, but not OpenSCAD's values
  ```

## Closed since M0

The current gates are `corpus/echo` **35/35**, geometry **125/125**, and BOSL2
**511/513** with two explicit expected failures. Individual closures below state
their oracle or regression evidence where relevant:

- **`linear_extrude` combining twist with a non-uniform scale.** This was the
  last known silent geometry difference (0.8% high in volume). Two rules were
  identified black-box against the oracle and both now hold. **Refinement:** each
  profile edge is budgeted by the *peak stretch* its direction reaches over the
  swept slices — `max_t |diag(sx(t), sy(t)) · Rot(t·twist) · d|` sampled at the
  `slices+1` layers actually emitted (a coarse `slices=` shortens it). This one
  rule subsumes the two already-closed regimes: pure twist gives the edge's own
  length (rotation is isometric) and pure non-uniform scale gives
  `max(original, scaled)`. **Sweep:** the layer transform is
  *rotate-then-scale*, applying the non-uniform scale in the fixed frame, so a
  corner swung toward an axis picks up that axis's factor (the r≈13 excursion the
  old rotate-of-a-scaled-profile could never reach). Uniform scale commutes with
  the rotation and pure twist leaves the scale at 1, so only the combined case
  moved. Gated by `corpus/geom/ext_linear_twist_scale*.scad` (positive/negative
  twist, shrink, and a holed profile) against the STL oracle.

  ```scad
  linear_extrude(height=7, twist=200, scale=[0.4,1.6], center=true) square([8,5]);
  ```

- **An inside-out `polyhedron` adds instead of subtracting.** OpenSCAD winds a
  face clockwise seen from outside; the opposite winding makes the solid
  inside-out. A lone one exports with reversed normals in both engines, so it
  looks fine — but once it reaches the CSG kernel upstream treats it as an
  ordinary positive solid, where OpenRSCAD treated it as a negative one. A union
  *lost* the volume (28.33 → 25.67 on a tetrahedron plus a cube) and a difference
  cut nothing at all. Orientation is now normalised at the single point a mesh
  enters the kernel, in both the native C++ and pure-Rust backends, so imported
  meshes get the same repair. Gated by `corpus/geom/polyhedron_winding.scad`.

  ```scad
  // faces given the wrong way round; still adds, as upstream
  polyhedron([[0,0,0],[2,0,0],[0,2,0],[0,0,2]], [[0,2,1],[0,1,3],[1,2,3],[0,3,2]]);
  translate([6,0,0]) cube(3);
  ```

- **`surface()` reads PNG heightfields, and inverts them the way upstream does.**
  `invert=true` maps grey `g` to `(1 - g)/2.55`, not the `(255 - g)/2.55` a
  reader would expect: upstream inverts the sample against normalised white
  while dividing a 0–255 byte. The relief is identical either way — the two
  differ by a constant `254/2.55` in z — so matching it costs nothing in shape
  and makes the placement exact. `invert` on a `.dat` heightfield is a no-op in
  both engines. Gated by `corpus/geom/surface_png.scad` and `surface_options.scad`.

  ```scad
  surface("heights.png", invert = true);   // z in [-79.04, 0.39], as upstream
  ```

- **`$vpf` defaults to 22.5.** It was 45 — the number a renderer would assume
  for a field of view, and not what upstream reports. `$t`, `$vpr`, `$vpt` and
  `$vpd` were already right. Gated by `corpus/echo/operators_math.scad`.

- **`.csg` tree export works.** `-o out.csg` serializes the evaluated model the
  way OpenSCAD does: every module call resolved, every expression evaluated, and
  every transform lowered to a `multmatrix`. It needs no render, so it is
  produced straight from the tree and keeps `$preview` true, as upstream does.
  The contract is a *round trip* rather than byte-identical text — OpenSCAD
  omits parameters left at their defaults and the IR does not record which were
  written, so we write them all, which is equally valid input. Re-rendering our
  output reproduces the geometry in both engines across 19 constructs; the
  in-repo test also asserts a second export is a fixed point.

  ```sh
  openrscad -o model.csg model.scad && openscad -o from-csg.stl model.csg
  ```

- **`text()` is shaped, not summed.** Runs go through `rustybuzz`, the Rust
  port of the same HarfBuzz shaper OpenSCAD uses, so kerning pairs, ligatures
  and joining scripts come out right instead of being approximated glyph by
  glyph: `"AV"` was 1mm too wide, `"ffl"` missed its ligature, and Arabic
  rendered *nothing at all*. `direction=` selects `ltr`/`rtl`/`ttb`/`btt`, with
  `script=`/`language=` passed through.

  Three details were measured rather than assumed, each now oracle-gated: a
  vertical run centres every glyph in a slot the height of the OS/2 typographic
  ascent-to-descent span *and* centres it on the column, rather than running a
  baseline; `valign` aligns the **ink** box, not the font's ascender, so `"aaa"`
  and `"Hqp"` sit differently under `valign="top"`; and glyph curves flatten to
  four segments by default, where eight put a plain `o` 1.2% over upstream's
  area and four lands within 0.02%.

  ```scad
  text("AV");                                    // kerned
  text("مرحبا", direction="rtl", script="arabic");  // joined, was empty
  text("abc", direction="ttb");                  // stacked and centred
  ```

- **AMF and 3MF objects keep their own index spaces.** Triangle indices are
  numbered per `<object>`; reading a whole file into one list made a second
  object's faces address the first object's points, so a package of a 2mm and a
  3mm cube imported as two 2mm cubes. 3MF `<build>` items are now assembled with
  their transforms. AMF `unit` and `<constellation>` are ignored, which is what
  upstream does — an AMF in inches imports at the same size there.

- **Import selectors and placement are honoured.** `layer=` keeps a single DXF
  layer or Inkscape SVG layer, `id=` selects any SVG element by id, and
  `origin`/`scale` place a 2D import as `(point - origin) * scale` — all four
  were previously accepted and silently ignored, so a selective import returned
  the whole drawing. `origin`/`scale` remain 2D-only, as upstream.

  ```scad
  import("part.dxf", layer="outline", origin=[1,1], scale=2);
  import("art.svg", id="badge");
  ```

- **The retained deprecated 2021.01 forms are implemented.** `assign`, `child`,
  `import_stl`, `import_dxf`, `dxf_dim`, and `dxf_cross` all work, each with the
  deprecation notice OpenSCAD prints. Two behaviours are easy to get wrong and
  are pinned by oracle cases: `assign()` is *not* `let()` — every right-hand
  side evaluates in the enclosing scope and the bindings land together, so with
  `x = 100`, `assign(x = 1, y = x + 1)` gives `y == 101` — and bare `child()` is
  the *first* child alone where bare `children()` is all of them.

  ```scad
  x = 100; assign(x = 1, y = x + 1) echo(x, y);   // 1, 101
  module m() { child(); } m() { cube(2); cube(9); }  // just the cube(2)
  echo(dxf_dim(file="part.dxf", name="width"), dxf_cross(file="part.dxf", layer="marks"));
  ```

- **Non-planar extrude walls are split along the shorter diagonal.** A twisted
  or non-uniformly scaled wall quad is not planar, so its two diagonals enclose
  different volumes — on a 32-gon twisted by one vertex step per slice, one
  reproduces the prism exactly and the other cuts 1.3% off. OpenSCAD picks the
  shorter one per quad, falling back to the wall's lean (the twist direction,
  flipped for a hole) only when the two are exactly equal, which is the common
  case on symmetric profiles. Matched per quad against the oracle over 3142
  quads. This closed the last three known silent geometry differences at once:
  holed profiles (was 0.6% high), profiles translated off the Z axis (1.3%), and
  non-uniformly scaled curved profiles (0.13%).

  ```scad
  linear_extrude(height=10, twist=90) difference(){ square(10); translate([3,3]) square(4); }
  linear_extrude(height=10, twist=90, slices=4) translate([20,0]) square(10);
  ```

- **Non-uniform `scale` refines the profile and adds slices.** OpenSCAD treats
  a non-uniform scale like a twist: the outline is re-tessellated and extra
  slices are swept, while a uniform scale keeps every wall planar and refines
  nothing however far from 1 it is. An edge earns its share of the segment
  budget by `max(original, scaled)` length, and the slice count comes from how
  far the worst-placed profile point travels to its scaled position,
  `ceil(hypot(travel, height) / $fs)`, with `$fn` replacing it outright. This is
  volume-neutral on its own, so the two corpus cases pin `tris` as well —
  596 triangles for the scaled square, 12 for the uniform frustum.

  ```scad
  linear_extrude(height=10, scale=[0.2,2]) square(10);  // 596 triangles, as upstream
  linear_extrude(height=10, scale=0.5) square(10);      // 12, unrefined
  ```

- **Unusable export suffixes are rejected.** An unrecognized or missing `-o`
  suffix now exits non-zero and writes nothing, matching OpenSCAD, instead of
  silently emitting binary STL under whatever name was asked for. Validation
  happens before evaluation, so a typo costs nothing on a heavy model, and
  recognized suffixes match case-insensitively (`out.STL` is an STL).

  ```sh
  openrscad -o out.foo model.scad   # error: invalid output suffix 'foo'
  openrscad -o out.csg model.scad   # error: CSG tree export is not supported yet
  ```

- **Non-positive dimensions yield no geometry.** Zero or negative `cube`/`square`
  components, `sphere`/`circle` radii, `cylinder` height or radii, and
  `linear_extrude` height now produce nothing, matching upstream; a single zero
  `cylinder` radius is still a valid cone, and an extrude's children are still
  evaluated so their `echo`/`assert` side effects run. This was worse than a
  zero-volume result: the degenerate triangles were non-manifold, so
  `difference(){ cube(10); cube(0); }` failed in the CSG kernel and fell back to
  un-combined geometry. Gated by `corpus/geom/prim_invalid_dims.scad`.

  ```scad
  cube([-2,3,4]); cylinder(h=-5, r=2); linear_extrude(height=-5) square(2);
  cylinder(h=5, r1=0, r2=3);   // still a cone
  ```

- **`$` arguments are dynamically scoped over the callee and its children.**
  `linear_extrude($fn=32) circle(5)` now gives the circle 32 fragments instead
  of resolving it from `$fa`/`$fs`, and the same holds for builtin modules, user
  modules and their forwarded `children()`, and function calls (where a `$`
  argument is not a declared parameter and was previously dropped). Each
  argument expression still evaluates exactly once, in the caller's scope, so
  `m($fa=$fa/2)` halves rather than compounds and `m($fn=7, $fa=$fn)` reads the
  caller's `$fn`; nothing leaks past the call. This retired two BOSL2 expected
  failures (both `test_segs`), taking that gate from 503/513 to 505/513. Gated by
  `corpus/echo/special_args.scad` and `corpus/geom/special_args_fn.scad`.

  ```scad
  linear_extrude(height=1, $fn=32) circle(5);  // 32-gon, as upstream
  function f(x) = x * $fn; echo(f(2, $fn=10)); // 20
  ```

- **`linear_extrude` refinement under twist.** `segments=` is honoured, the
  implicit slice count follows `$fn`/`$fa`/`$fs`, and the 2D profile is
  re-tessellated before a twisted sweep. Previously these read 6–16% high in
  volume; across an 18-case matrix, cases outside the 0.1% oracle tolerance went
  from 15/18 to 2/18 (the rest is tracked as the wall-diagonal and non-uniform
  scale entries above). The rules, derived black-box from OpenSCAD 2024.12.17:
  each contour gets a segment budget (`segments=`, else `$fn`, else `360/$fa`)
  apportioned across its edges by length, with a one-segment floor per edge and
  a `ceil(len/$fs)` per-edge cap; the slice count is the tighter of the `$fa`
  per-slice twist limit and the `$fs` helical-travel limit. Gated by
  `corpus/geom/ext_linear_twist*.scad` and `ext_linear_segments*.scad`.

  ```scad
  linear_extrude(height=10, twist=90, slices=3, $fa=3, $fs=0.5) square(10);
  ```

- **`$preview` follows the render path.** Evaluation mode is now an explicit
  input rather than a hardcoded `true`, so a script that branches on `$preview`
  gets the branch matching the work actually being done. Measured against
  OpenSCAD 2024.12.17, which reports `false` exactly when an exact render
  happens: mesh export, 2D vector (DXF/SVG) export, and `--render`; and `true`
  for F5 preview, PNG rasters, and echo-only runs. Both directions are gated —
  `corpus/geom/preview_branch.scad` pins the exact side against the binary-STL
  oracle and `corpus/echo/preview_mode.scad` pins the preview side against the
  echo oracle. The CLI gains OpenSCAD's `--render`/`--preview` overrides, and
  `-D '$preview=…'` still wins over both.

  ```scad
  if ($preview) sphere(20); else cube(10); // an STL export is now the cube
  ```

- **Initial Track F closures.** Named `multmatrix`, positional cylinder/text
  arguments, `intersection_for`, `$parent_modules`/`parent_module()`, raw
  punctuation-preserving include/use paths, and the confirmed NaN/iteration/
  reducer/`chr`/`version_num` edges are fixed and regression-covered.
  `rotate_extrude` now handles negative-side profiles, axis crossings, partial
  fragment counts, zero sweeps, and angles over 360 degrees. Bare and
  display-wrapped `projection()` now export through the kernel-aware DXF/SVG
  path on CLI, wasm/npm, LSP, and desktop. Omitted function/module defaults now
  evaluate lazily in definition scope for ordinary variables while retaining
  dynamic `$` variables, across tree-walk, VM, and module paths. General planar
  concave `polyhedron` faces use projected earcut triangulation instead of an
  overlapping fan.

- **Assignment hoisting (last-write-wins).** Within a scope, only a variable's
  *final* assignment is evaluated, at the point it was *first introduced*. A read
  of a variable that is reassigned later sees the final value, not the
  intermediate one (`p = 1; q = p; p = 5;` → `q == 5`), and an overwritten
  assignment's RHS is discarded entirely, side effects included. There are **no
  forward references**: a read of a variable introduced later in the scope does
  not see it and falls through to an outer binding or `undef`
  (`y = x; x = 5;` → `y == undef` at top level; the nested case reads the outer
  binding). Earlier docs described the target as full forward-reference
  resolution — the OpenSCAD oracle showed that is *not* how upstream behaves, so
  this now matches observed OpenSCAD 2024.12 (`corpus/echo/assign_hoist.scad`).
  A reassigned name emits a spanned "assigned again … overwritten" lint on the
  dead write (deduped per source site; suppressed for `include`d/`use`d library
  code). The companion "Ignoring unknown variable" lint is **deferred**: library
  helpers evaluate in the caller's statement context and legitimately read unset
  parameters as `undef`, so warning on unbound reads produced false positives on
  real BOSL2 code — closing it needs read-site provenance the AST does not yet
  carry.

- **Oblique / offset extrudes.** `linear_extrude(height=h, v=[x,y,z])` sweeps
  the profile a distance `h` along `normalize(v)` (an oblique prism); oracle-gated
  by `corpus/geom/ext_linear_v*.scad`. `rotate_extrude(angle=a, start=s)` sweeps
  from `s` to `s + a` about Z — equivalent to the `[0, a]` sweep rotated by `s`.
  The 2024.12 oracle predates `start=` (it warns "variable start not specified as
  parameter"), so `start=` is verified by rigid-motion invariant (volume
  preserved, geometry rotated) rather than an echo/geom golden.

- Geometry breadth — the full 2D/3D primitive, transform, extrude, `hull`,
  `minkowski`, and `offset` surface shipped in M3 (was M0's cube/sphere/cylinder
  + booleans only).
- Echo number formatting (`%.6g`, stripped exponents); list comprehensions
  (`for`/`if`/`let`/`each`, nested, and the C-style 2019.05
  `for(init;cond;update)` form); string quoting in `echo` + `str`/`chr`/`ord`/
  string-indexing; `search`/`lookup`/`is_*`; module `children()`/`$children`;
  function literals; **lexical scoping** for ordinary variables/functions/
  modules with dynamic scoping for `$` variables; and the `polyhedron`
  primitive.

## Compatibility bar

- No bug-for-bug reproduction of CGAL/Nef coincident-face degeneracies.
- Geometry is compared within the documented oracle tolerances, not by vertex
  order or identical mesh bytes.
