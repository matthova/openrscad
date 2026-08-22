// Retained deprecated 2021.01 language forms.
//
// `assign()` binds in a new scope, but unlike `let()` every right-hand side is
// evaluated in the *enclosing* scope and the bindings land together.
x = 100;
assign(x = 1, y = x + 1) echo("assign", x, y);   // 1, 101 -- let would give 1, 2
let(x = 1, y = x + 1) echo("let", x, y);
echo("after", x);                                 // bindings do not escape
assign() echo("no bindings");
assign(a = 1) assign(b = 2) echo("nested", a, b);

// Bare `child()` is the first child, where bare `children()` is all of them.
module bare_child() { child(); }
module indexed_child() { child(1); }
module all_children() { children(); }
bare_child()   { echo("first"); echo("second"); }
indexed_child(){ echo("k0"); echo("k1"); }
all_children() { echo("c0"); echo("c1"); }
module counts() { echo("children", $children); child(0); }
counts() { echo("a"); echo("b"); }
