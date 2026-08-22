// The *statement* forms of the control constructs, which are distinct surfaces
// from the expression forms already covered by ternary_let and comprehension:
// `let(...) body` as a statement, `for (...) body` as a statement, `if/else`,
// and a bare block.

// if / else, including the chained form.
if (1 < 2) echo("if-taken"); else echo("if-not");
if (1 > 2) echo("else-not"); else echo("else-taken");
if (false) echo("a"); else if (true) echo("chained"); else echo("c");

// Statement `for`: a range, a list, a stepped range, and the Cartesian product
// of two variables in one header.
for (i = [0:2]) echo("range", i);
for (v = [7, "x", [1,2]]) echo("list", v);
for (i = [0:2:6]) echo("step", i);
for (i = [0:1], j = ["a","b"]) echo("cartesian", i, j);

// Statement `let` — a new scope whose bindings are sequential, unlike assign().
q = 100;
let (q = 1, r = q + 1) echo("let", q, r);   // 1, 2: r sees the new q
echo("after-let", q);

// A bare block is transparent to the enclosing scope for geometry, but its
// assignments are local to it.
{ z = 5; echo("block", z); }

/* A block comment
   spanning lines. */ echo("comments", 1);  // and a line comment
echo("comment-in-expr", 1 + /* inline */ 2);

// Argument binding: positional, named, and the two mixed, with named winning.
module bind(a, b, c) { echo("bind", a, b, c); }
bind(1, 2, 3);
bind(a = 1, b = 2, c = 3);
bind(1, c = 3, b = 2);
bind(1, 2);                 // c is undef
function fbind(a, b = 9) = [a, b];
echo("fn-bind", fbind(1), fbind(1, 2), fbind(b = 5, a = 4));
