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

- **Extrudes with non-planar walls triangulate them differently in three
  cases.** A twisted or non-uniformly scaled wall quad is non-planar, so its two
  diagonals enclose different volumes. The split follows the twist direction and
  contour winding — exact for twisted profiles that straddle the Z axis, still
  0.6% high for a hole, 1.3% for a profile translated off the axis, and 0.13%
  for a non-uniformly scaled curved profile. In every one of these the vertex
  positions, slice count and triangle count match OpenSCAD exactly; only which
  diagonal splits each quad differs.

  ```scad
  linear_extrude(height=10, twist=90) difference(){ square(10); translate([3,3]) square(4); }
  linear_extrude(height=10, twist=90, slices=4) translate([20,0]) square(10);
  $fn=24; linear_extrude(height=10, scale=[0.2,2]) circle(5);
  ```

## Missing or partial compatibility

- **Deprecated compatibility aliases remain undecided.** OpenSCAD 2021.01 still
  accepts legacy `assign`, `child`, `import_dxf`, `import_stl`, `dxf_dim`, and
  `dxf_cross` forms; OpenRSCAD does not currently implement them.

- **Text font discovery is broad, but shaping is partial.** Native hosts scan
  installed fonts; Chromium can load permission-granted local fonts; the bundled
  Liberation family remains the deterministic fallback. Layout is still
  codepoint-by-codepoint: kerning, ligatures, complex-script shaping, vertical
  directions, and meaningful `language`/`script` selection are absent, and RTL
  merely reverses codepoints. An unavailable family warns and falls back.

  ```scad
  text("office", font="Liberation Serif"); // no ligature/kerning shaping yet
  text("مرحبا", direction="rtl", language="ar", script="arabic");
  ```

- **Import parsers accept the headline formats, not every documented construct.**
  DXF/SVG selectors and options (`layer`, `id`, transforms/DPI), SVG nesting,
  transforms, use/style/visibility, DXF bulges/splines/ellipses, and caller curve
  resolution remain incomplete. 3MF/AMF import does not yet assemble units,
  independent object index spaces, components, or build-item transforms.

- **OpenSCAD-style CSG tree export is absent.** OpenRSCAD can export rendered
  mesh and vector formats, but it does not serialize the evaluated model as a
  `.csg` operation tree. `-o out.csg` now fails with a format-specific error
  rather than silently writing STL bytes under that name, so the gap is loud.

- **BOSL2 function-suite coverage is partial and gated.** `xtask bosl2` passes
  505/513 pinned blocks across 15 files. The expected failures are
  `test_gaussian_rands`, `test_format`, `test_format_float`, `test_str_strip`,
  `test_hstack`, `test_typeof`, `test_f_acos`, and `test_struct_val`. This is
  broad library evidence, not the complete BOSL2 module suite.

## Warned divergences

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

- **`rands()` is not bit-compatible.** OpenRSCAD uses an xorshift PRNG; values
  are reproducible and global/seeded advance semantics match, but the sequence
  differs from OpenSCAD's generator. This is intentional.

  ```scad
  echo(rands(0, 1, 3, seed=42)); // reproducible, but not OpenSCAD's values
  ```

## Closed since M0

The current gates are `corpus/echo` **27/27**, geometry **94/94**, and BOSL2
**505/513** with eight explicit expected failures. Individual closures below state
their oracle or regression evidence where relevant:

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
