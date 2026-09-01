# OpenRSCAD roadmap

_Status snapshot: 2026-08-09. M0–M9 are complete. Tracks A–E are retained as
historical design/implementation records; their line references and test counts
describe the commits at which they were written, not the current tree. Tracks F and G are
active._

| track | doc | theme | effort | exit criterion (summary) |
|---|---|---|---|---|
| **A (M6, complete)** | [track-a-trustworthy-geometry.md](track-a-trustworthy-geometry.md) | Geometry oracle and known silent-geometry fixes | shipped | Gates when Track A shipped: `xtask geom` 81/81, `xtask echo` 25/25, `xtask bosl2` 503/513; now 91/91, 27/27, 505/513 |
| **B (M7, complete)** | [track-b-switcher-experience.md](track-b-switcher-experience.md) | Desktop/project workflow, diagnostics, color, PNG, presets, animation | shipped | Daily switcher workflow exists across desktop and CLI |
| **C (complete)** | [track-c-ci-hardening.md](track-c-ci-hardening.md) | CI, fuzzing, wasm, desktop, web, and oracle gates | shipped incrementally | Retained as a historical audit; current workflow files are authoritative |
| **D (M8, complete)** | [track-d-ui-structure.md](track-d-ui-structure.md) | UI structure, quality, inspector, integrity | shipped | Historical implementation plan |
| **E (M9, complete)** | [track-e-ui-ceiling.md](track-e-ui-ceiling.md) | Responsive command registry and product ceiling | shipped | Historical implementation plan |
| **F (M10, complete)** | [track-f-measured-openscad-compatibility.md](track-f-measured-openscad-compatibility.md) | Classify and close the full OpenSCAD compatibility surface | incremental | Every 2021.01 core feature classified and tested; zero known silent differences |
| **G (M11, active)** | [track-g-parity-beyond-the-language-core.md](track-g-parity-beyond-the-language-core.md) | Parity beyond the language core: CLI/workflow, experimental surface, geometry gaps, libraries, GUI, LSP | incremental | `openrscad` is a drop-in for `openscad` in scripts and CI, and the app covers the OpenSCAD GUI feature set |

## Why Track F is next

The existing oracles prove representative cases, not the complete documented
surface. A black-box audit found unsupported constructs and silently ignored
parameters outside those cases. Track F replaces broad coverage claims with a
versioned manifest, then closes the highest-impact silent differences first.

## Constraints that apply to all tracks

- **Clean-room policy** (CONTRIBUTING.md): never read OpenSCAD source. All
  compat work is black-box — user manual, Wikibook, and observed behavior of
  the `openscad` binary (echo output, exported meshes). Oracle testing is
  inherently clean-room-safe: it compares outputs, not implementations.
- **Dual kernels**: every geometry change must work on both the native C++
  Manifold backend and the pure-Rust Manifold wasm backend (`Kernel` trait,
  `crates/openrscad-geom/src/lib.rs`), and keep the existing differential test
  passing.
- **The tree-walk interpreter is the reference semantics**; the bytecode VM
  must never change results, only timing.
- **Oracle machine**: OpenSCAD 2024.12 lives at `/opt/homebrew/bin/openscad`
  on the dev machine. CI must not require it — golden files are blessed
  locally and committed (same model as the echo oracle).
