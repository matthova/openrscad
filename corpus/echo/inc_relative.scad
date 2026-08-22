// A nested relative include: this file pulls in sub/outer.scad, which itself
// includes a sibling as <dep.scad>. That inner path resolves relative to the
// file doing the including, so it finds sub/dep.scad rather than failing.
include <sub/outer.scad>
echo(OUTER, DEP, tripled(7));
