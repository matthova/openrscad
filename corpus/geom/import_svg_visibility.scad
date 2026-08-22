// Three rules that are easy to assume wrong, all measured against 2024.12:
//   * `<defs>` draws nothing (it holds templates for `<use>`), and selecting an
//     element inside it by id still draws nothing;
//   * `display:none` hides, as an attribute or in `style=`;
//   * `visibility:hidden` does *not* hide — upstream renders it regardless.
// So of the six rects here only three come through: the plain one and the two
// marked visibility:hidden.
//
// oracle: tris
linear_extrude(1) import("visibility.svg");
