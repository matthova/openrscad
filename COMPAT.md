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

- **Invalid dimensions create solids instead of empty geometry.** Negative
  cube/square dimensions, cylinder height/radii, and extrusion height are not
  rejected consistently.

  ```scad
  cube([-2,3,4]);
  cylinder(h=-5, r=2);
  linear_extrude(height=-5) square(2);
  ```

- **Linear-extrusion refinement differs.** `linear_extrude(segments=)` is ignored,
  omitted `slices` does not follow `$fn`, and the 2D profile is never re-tessellated
  under twist. The last part is not avoided by pinning `slices`: with
  `slices=3` fixed, OpenSCAD's volume moves with `$fn`/`$fa`/`$fs` (988.7 → 972.0 →
  963.7) while OpenRSCAD stays at 1122.0, up to 16% high.

  ```scad
  linear_extrude(height=10, twist=90, $fn=40) square(10);     // 1001.1 vs 1074.9
  linear_extrude(height=10, twist=90, slices=3, $fa=3, $fs=0.5) square(10);
  ```

- **Unsupported export suffixes silently produce binary STL.** `-o out.csg` writes
  STL bytes named `.csg` and exits 0 rather than serializing a CSG tree, and any
  unrecognized suffix does the same; OpenSCAD rejects an invalid suffix outright.

  ```sh
  openrscad -o out.csg model.scad   # binary STL, no warning
  openrscad -o out.foo model.scad   # binary STL, no warning
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
  `.csg` operation tree.

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

The current gates are `corpus/echo` **26/26**, geometry **82/82**, and BOSL2
**503/513** with ten explicit expected failures. Individual closures below state
their oracle or regression evidence where relevant:

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
