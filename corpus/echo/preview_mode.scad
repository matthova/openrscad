// The other side of `corpus/geom/preview_branch.scad`: a run that performs no
// exact render reports `$preview == true`. The echo oracle is
// `--export-format=echo`, which renders nothing, so this pins the preview mode
// while the geom case pins the exact one.
echo("preview", $preview);
echo("branch", $preview ? "f5" : "f6");

// A `$preview`-driven refinement switch — the common real-world use — must pick
// the preview value here.
$fn = $preview ? 12 : 96;
echo("fn", $fn);
