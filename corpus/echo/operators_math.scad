// Vector and matrix arithmetic, and the trig entry points that the committed
// math case does not reach (it exercises atan2, not atan).
a = [1, 2, 3];
b = [4, 5, 6];
echo("add", a + b);
echo("sub", a - b);
echo("neg", -a);
echo("dot", a * b);              // vector * vector is the dot product
echo("scale", a * 2, 2 * a, a / 2);

m = [[1, 2], [3, 4]];
n = [[5, 6], [7, 8]];
echo("matmul", m * n);           // matrix * matrix
echo("matvec", m * [1, 1]);      // matrix * vector
echo("vecmat", [1, 1] * m);      // vector * matrix
echo("matadd", m + n);

echo("atan", atan(1), atan(-1), atan(0), atan(1e9));
echo("$t", $t);
echo("viewport", $vpr, $vpt, $vpd, $vpf);
