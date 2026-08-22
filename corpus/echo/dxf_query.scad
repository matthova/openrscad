// The deprecated DXF readers. `dxf_dim` matches on the dimension *text*
// (group 1), not the block name, and recomputes the measurement from the
// definition points rather than trusting the stored group 42. Its positional
// order is upstream's, which is not the order the names suggest — `name` comes
// last, after `origin` and `scale`.
echo("rot0",    dxf_dim(file="dims.dxf", name="rot0"));      // projects onto 0 deg
echo("rot90",   dxf_dim(file="dims.dxf", name="rot90"));     // onto 90 deg
echo("rot45",   dxf_dim(file="dims.dxf", name="rot45"));     // onto 45 deg
echo("aligned", dxf_dim(file="dims.dxf", name="aligned"));   // plain distance
echo("flagged", dxf_dim(file="dims.dxf", name="flagged"));   // presentation bits masked off
echo("radius",  dxf_dim(file="dims.dxf", name="radius"));    // centre to chord
echo("first",   dxf_dim(file="dims.dxf"));                   // no name: the first one
echo("scaled",  dxf_dim(file="dims.dxf", name="rot0", scale=2));
echo("origin-is-ignored-for-a-length", dxf_dim(file="dims.dxf", name="rot0", origin=[1,0]));
echo("layered", dxf_dim(file="dims.dxf", name="onE", layer="E"));
echo("wrong-layer", dxf_dim(file="dims.dxf", name="onE", layer="D"));
echo("no-such-name", dxf_dim(file="dims.dxf", name="nope"));
echo("no-such-file", dxf_dim(file="missing.dxf", name="rot0"));
echo("positional", dxf_dim("dims.dxf", "D", [0,0], 3, "rot90"));

// `dxf_cross` returns the intersection of two lines, mapped through
// (point - origin) * scale. An empty layer selector matches any layer.
echo("cross",        dxf_cross(file="cross.dxf", layer="L1"));
echo("cross-anylayer",dxf_cross(file="cross.dxf"));
echo("cross-origin", dxf_cross(file="cross.dxf", layer="L1", origin=[1,2]));
echo("cross-scale",  dxf_cross(file="cross.dxf", layer="L1", scale=2));
echo("cross-both",   dxf_cross(file="cross.dxf", layer="L1", origin=[1,2], scale=2));
echo("cross-one-line", dxf_cross(file="cross.dxf", layer="L2"));
echo("cross-no-file", dxf_cross(file="missing.dxf"));
