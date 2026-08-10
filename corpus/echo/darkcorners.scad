echo(2+3*4, (2+3)*4, 2-3-4, 12/2/3, 2*3%4);
echo(-7%3, 7%-3, -7%-3);
echo(-(-5), !!true, !0, -[1,-2,3]);
echo(undef+1, undef*2, 1/undef, undef<1, undef==undef, undef==1);
echo(1&&2, 0||3, 5&&0, !"", !"x");
echo(len(""), str(), str(1,2,3));
echo([1,2,3][10], [][0], len([]));
echo(1?2?3:4:5, 0?1:0?2:3);
echo(1e21, 1e-10, 99999, 999999, 9999999);
echo([each [], 1, each [2,3]]);
echo(concat([1,[2,3]],[4]));
echo(max([]), min([1]));

// Type/iteration edges that are easy to accidentally inherit from Rust rather
// than OpenSCAD. The undef loop intentionally emits no line.
echo((0/0) ? "nan-true" : "nan-false");
for (i=undef) echo("unexpected", i);
echo([10,20][0/0]);
echo(min([1,"x",3]), max([1,undef,3]), norm([3,"x",4]));
echo(chr(65,66,67));
echo(version_num([1,2,3]), version_num([2024,12,17]));
