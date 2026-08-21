// A `$` argument is dynamically scoped over the callee and everything under it,
// for both module and function calls. Every argument still evaluates once, in
// the caller's scope, and nothing leaks past the call.

// Scopes into a builtin's children, however deep.
$fa = 12;
linear_extrude(height=1, $fa=$fa/2) echo("child", $fa);
echo("after", $fa);

// User modules, their bodies, and forwarded children().
module m() { echo("body", $fn); children(); }
m($fn=7) echo("forwarded", $fn);
echo("still", $fn);

// Every argument evaluates in the caller's scope, so a later one does not see
// an earlier `$` argument from the same call.
$fn = 0;
module two() { echo("two", $fn, $fa); }
two($fn=7, $fa=$fn);

// Function calls: a `$` argument is not a declared parameter but the body reads
// it, and it does not leak.
function f(x) = x * $fn;
echo("fn", f(2, $fn=10), $fn);

// Nested function calls inherit it dynamically.
function inner() = $fn;
function outer() = inner();
echo("nested", outer($fn=9));

// Argument expressions run exactly once.
module once() { echo("ran"); }
once($fn = echo("arg-evaluated") 8);
