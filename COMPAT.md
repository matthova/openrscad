# Compatibility divergence register

The release-blocking baseline is the stable language and modeling surface of
OpenSCAD 2021.01. Later stable features are tracked separately; experimental
features and GUI pixel parity are not implied. Mesh compatibility is measured by
geometry (volume, bounds, centroid, topology), not identical bytes or triangle
ordering. The full closure plan and scope are in
[Track F](docs/roadmap/track-f-measured-openscad-compatibility.md).

This register records confirmed differences. A silently wrong answer is a trust
bug, so silent entries stay here until fixed; intentional differences must warn
at runtime or be justified as permanent.

## Known silent differences

- **Default arguments have the wrong scope and are evaluated eagerly.** A user
  module/function default currently reads ordinary variables at the call site,
  rather than the definition's lexical environment, and runs even when the
  caller supplied that argument.

  ```scad
  x=10; module m(a=x) { echo(a); } module caller() { x=20; m(); } caller();
  function f(a=echo("default") 3)=a; echo(f(6));
  // OpenSCAD: 10, then 6 without "default". OpenRSCAD: 20 and an extra echo.
  ```

- **Exact renders still evaluate `$preview` as `true`.** The evaluator seeds the
  value independently of the selected render path, so exact exports can choose a
  preview-only model branch.

  ```scad
  if ($preview) sphere(20); else cube(10);
  // An exact STL export should contain the cube.
  ```

- **Some documented builtin argument forms are ignored.** Confirmed cases are a
  named matrix and the fourth positional cylinder argument; later positional
  `text()` layout arguments are also not bound.

  ```scad
  multmatrix(m=[[1,0,0,7],[0,1,0,11],[0,0,1,13],[0,0,0,1]]) cube(2);
  cylinder(10, 5, 3, true); // should span z=-5..5
  ```

- **Invalid dimensions create solids instead of empty geometry.** Negative
  cube/square dimensions, cylinder height/radii, and extrusion height are not
  rejected consistently.

  ```scad
  cube([-2,3,4]);
  cylinder(h=-5, r=2);
  linear_extrude(height=-5) square(2);
  ```

- **Extrusion refinement/sweep edge cases differ.** `linear_extrude(segments=)`
  is ignored and omitted `slices` does not follow `$fn`. `rotate_extrude()`
  mishandles wholly negative-X profiles, profiles crossing X=0, partial-sweep
  fragment counts, and angles over 360 degrees.

  ```scad
  linear_extrude(height=10, twist=90, $fn=40) square(10);
  rotate_extrude(angle=90, $fn=24) translate([5,0]) square(2);
  ```

- **Concave `polyhedron` faces are fan-triangulated.** This is only correct for
  convex or suitably star-shaped faces; a general concave face can be filled
  incorrectly.

- **A bare `projection()` is dropped by DXF/SVG export.** Projection under other
  2D operations is lowered correctly, but the kernel-free vector-export path
  produces empty contours for a bare projection.

  ```scad
  projection(cut=false) rotate([20,30,0]) cube(10);
  ```

- **Several evaluator edge cases differ.** Confirmed examples include NaN
  truthiness/indexing, iteration over `undef`, invalid members passed to
  `min`/`max`/`norm`, multi-argument `chr`, and `version_num(vector)`.

## Missing or partial compatibility

- **`intersection_for`, `$parent_modules`, and `parent_module(i)` are missing.**
  The first is offered by completion but evaluates as an unknown module; the
  module-stack introspection values are `undef`/unknown.

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

- **BOSL2 function-suite coverage is partial and gated.** `xtask bosl2` passes
  503/513 pinned blocks across 15 files. The expected failures are
  `test_gaussian_rands`, `test_format`, `test_format_float`, `test_str_strip`,
  `test_hstack`, `test_typeof`, two `test_segs` blocks, `test_f_acos`, and
  `test_struct_val`. This is broad library evidence, not the complete BOSL2
  module suite.

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

All oracle-checked; `corpus/echo` passes **25/25** and BOSL2's function suite
runs in the `xtask bosl2` harness:

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
