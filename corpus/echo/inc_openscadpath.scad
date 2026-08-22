// oracle: libs libpath
// `libpath/` is on OPENSCADPATH for this case, and is *not* a subdirectory the
// bare name would otherwise resolve to. A library not found next to the script
// is looked up there, so `<pathlib.scad>` resolves even though no such file
// sits beside this one.
use <pathlib.scad>
include <pathlib.scad>
echo(PATHLIB, halved(9));
