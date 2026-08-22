// The four modifier characters, which are geometry-visible in an export:
//   *  disables the subtree entirely
//   !  makes that subtree the whole model, discarding everything else
//   #  highlights but keeps the geometry
//   %  is background and is *excluded* from the rendered result
// Without `!` here the result would be the union of the cube and the sphere;
// the disabled and background parts must not appear either way.
cube(10);
*cube(100);              // disabled
#translate([12,0,0]) cube(4);   // highlighted, still exported
%translate([0,12,0]) cube(9);   // background, not exported
