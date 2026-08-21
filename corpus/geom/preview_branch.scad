// `$preview` must be false during an exact render/export (OpenSCAD's F6), so an
// export takes the `else` branch. The oracle runs `--export-format binstl`,
// which is an exact render; seeding `$preview = true` here would export a
// sphere of volume ~33500 instead of this cube's 1000.
if ($preview) sphere(20); else cube(10);
