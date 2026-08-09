module m() { echo($children); }
m() { cube(1); sphere(2); }
m();
m() { cube(1); }

echo("top", $parent_modules, parent_module(0));
module outer() { middle(); }
module middle() {
    echo("nested", $parent_modules,
         parent_module(0), parent_module(1), parent_module());
}
outer();
