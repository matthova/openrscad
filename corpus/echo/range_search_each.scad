// Builtin edge cases surfaced by BOSL2 (typeof, str_strip, struct_val):
//   - a range literal with a non-numeric bound or step is `undef`;
//   - a nan-stepped range still equals itself (so BOSL2 `is_nan(x)=x!=x` is
//     false for it);
//   - `each` over a string spreads its characters;
//   - `search` matches a list-valued key against column 0 or a whole row.

// Non-numeric range components collapse to undef; numeric ones (incl. nan) do not.
echo("range_str", [0:"a":10]);
echo("range_list", [0:[]:10]);
echo("range_bool", [true:1:10]);
echo("range_num", is_num([0:0/0:10][1]));   // the step is a number (nan)
r = [0:0/0:10];
echo("range_self_eq", r == r, r != r);

// each over a string yields its characters.
echo("each_string", [each "abc"]);
echo("each_mixed", [for (s = ["ab", "cd"]) each s]);

// search with a list-valued key: found via column 0 or a bare matching row.
st = [["Foo", 91], [[5, 4], 3], [7, 92]];
echo("search_listkey", search([[5, 4]], st));
echo("search_wholerow", search([[1, 2]], [[1, 2], [3, 4]]));
echo("search_chars", search("abe", "abcdabcd"));
