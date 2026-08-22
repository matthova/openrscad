// parts.amf holds two <volume>s in one <object> — a 2x3x4 and a 1x1x5 box, 24
// triangles between them — so it pins that volumes share the object's vertex
// list rather than restarting their index space. boxy.obj is a 3x4x5 box
// written as six quad faces with 1-based indices, so it pins face fan-out too.
import("parts.amf");
translate([0, 8, 0]) import("boxy.obj");
