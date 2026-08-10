# OpenSCAD compatibility manifest

`openscad-2021.01.json` is the machine-readable compatibility contract for the
stable OpenSCAD 2021.01 language and modeling surface. It is deliberately more
conservative than a builtin/completion list:

- `verified` means a committed OpenSCAD-derived echo or geometry oracle case is
  linked in `tests`;
- `implemented` means the surface exists, but its documented input space has not
  been established by an upstream oracle;
- `missing` includes parameters that are accepted but ignored or whose current
  result silently differs;
- `warned_divergence` is a deliberate difference with a runtime warning;
- `permanent_divergence` is an intentional, documented difference; and
- `unknown` means the surface has not yet been audited.

Entries are intentionally split when only part of a module is compatible. For
example, the oracle-covered `linear_extrude(height, center, twist, scale,
slices)` entry is separate from its missing `segments` and implicit `$fn`
refinement rules. A supported headline module therefore does not conceal an
unsupported parameter.

The baseline inventory comes from the public OpenSCAD 2021.01 cheat sheet and
user manual, then is classified against the OpenRSCAD evaluator, geometry
dispatch, committed corpus, and the Track F audit. Post-baseline surfaces may be
listed as `current_stable` when they are already implemented or materially
interact with the baseline; they do not change the 2021.01 exit criterion.
OpenRSCAD-only additions use `openrscad_extension` so they cannot be mistaken
for an OpenSCAD compatibility claim.

Run the dependency-free validator from the repository root:

```sh
python3 compatibility/validate.py
```

The validator checks the schema's important invariants, unique IDs, status
vocabulary, evidence requirements, and that every `echo:` / `geom:` oracle ID
resolves to both a source case and a committed golden.
