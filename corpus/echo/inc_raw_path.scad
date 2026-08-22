// An `include <...>` path is taken raw, up to the closing angle bracket: spaces,
// parentheses and `+` are all part of the filename, not tokens to be lexed. The
// path is not an expression and is not string-escaped.
include <sub/odd name (v2)+x.scad>
echo(ODD);
