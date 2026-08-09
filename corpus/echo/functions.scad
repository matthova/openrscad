f = function(x) x*x;
echo(f(5));
g = function(a,b) a+b;
echo(g(3,4));
echo(is_function(f), is_function(5));
echo([for(i=[1:4]) f(i)]);
h = function(n) n<=1 ? 1 : n*h(n-1);
echo(h(5));

// Defaults are lazy and lexical for ordinary variables, while `$` variables
// remain dynamic. Explicit arguments run first in the caller's scope.
lexical_default = 10;
$dyn_default = 1;
function default_fn(
    a=echo("fn-default") lexical_default,
    b=$dyn_default,
    c=echo("fn-dead") 3
) = [a,b,c];
module default_mod(a=echo("mod-default") lexical_default, b=$dyn_default) {
    echo("mod", a, b);
}
module default_caller() {
    lexical_default = 20;
    $dyn_default = 7;
    echo("fn", default_fn(c=echo("fn-arg") lexical_default));
    default_mod();
}
default_caller();
