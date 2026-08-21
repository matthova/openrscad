---
"openrscad-release-root": minor
---

`$fn`/`$fa`/`$fs` (and any `$` variable) passed as a call argument now reach the callee's children, matching OpenSCAD. `linear_extrude($fn=32) circle(5)` previously rendered the circle from `$fa`/`$fs` — a 16-gon instead of a 32-gon — and function calls such as `f(2, $fn=10)` dropped the argument entirely. Each argument expression still evaluates exactly once, in the caller's scope.
