import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { EditorState } from "@codemirror/state";
import { CompletionContext } from "@codemirror/autocomplete";
import { ensureSyntaxTree } from "@codemirror/language";
import { openscad } from "./openscad";
import { openscadCompletion } from "./complete";
import { setWorkspaceFiles } from "./workspaceFiles";

/** Build an OpenSCAD editor state, force a full parse, and run the completion
 *  source at `pos` (end of doc by default). `explicit` mimics Ctrl-Space. */
function complete(doc: string, pos = doc.length, explicit = true) {
  const state = EditorState.create({ doc, extensions: [openscad()] });
  ensureSyntaxTree(state, doc.length, 5000);
  return openscadCompletion(new CompletionContext(state, pos, explicit));
}

function labels(doc: string, pos?: number, explicit?: boolean): string[] {
  return complete(doc, pos, explicit)?.options.map((o) => o.label) ?? [];
}

describe("openscadCompletion", () => {
  it("offers builtin modules and functions", () => {
    const l = labels("");
    expect(l).toContain("cube");
    expect(l).toContain("cylinder");
    expect(l).toContain("sphere");
    expect(l).toContain("linear_extrude");
  });

  it("offers keywords and special variables", () => {
    const l = labels("");
    expect(l).toContain("module");
    expect(l).toContain("function");
    expect(l).toContain("$fn");
    expect(l).toContain("$vpr");
  });

  it("carries a builtin's signature as detail and doc as info", () => {
    const cube = complete("")?.options.find((o) => o.label === "cube");
    expect(cube?.type).toBe("function");
    expect(cube?.detail).toBe("cube(size, center=false)");
    expect(typeof cube?.info).toBe("string");
  });

  it("types special variables as variables", () => {
    const fn = complete("")?.options.find((o) => o.label === "$fn");
    expect(fn?.type).toBe("variable");
  });

  it("surfaces a user module with its parameter list as detail", () => {
    const widget = complete("module widget(a, b) {}\n")?.options.find(
      (o) => o.label === "widget",
    );
    expect(widget?.type).toBe("function");
    expect(widget?.detail).toBe("(a, b)");
  });

  it("surfaces user assignments and parameters as variables", () => {
    const opts = complete("size = 5;\nmodule m(depth) {}\n")?.options ?? [];
    expect(opts.find((o) => o.label === "size")?.type).toBe("variable");
    expect(opts.find((o) => o.label === "depth")?.type).toBe("variable");
  });

  it("dedups a user symbol over a same-named builtin (one entry, user wins)", () => {
    const opts = complete("module render() {}\n")?.options ?? [];
    const renders = opts.filter((o) => o.label === "render");
    expect(renders).toHaveLength(1);
    expect(renders[0].detail).toBe("()"); // the user's empty param list, not the builtin sig
  });

  it("filters by a `$` prefix so special vars trigger", () => {
    const r = complete("$f");
    expect(r).not.toBeNull();
    expect(r!.from).toBe(0); // completes from the start of `$f`
    expect(r!.options.some((o) => o.label === "$fn")).toBe(true);
  });

  it("returns null at an empty, non-explicit boundary", () => {
    expect(complete("", 0, false)).toBeNull();
  });

  it('offers bundled fonts inside a `font="…"` string', () => {
    const doc = 'text("hi", font="";';
    const r = complete(doc, doc.indexOf('"";') + 1); // cursor between the quotes
    expect(r).not.toBeNull();
    const l = r!.options.map((o) => o.label);
    expect(l).toContain("Liberation Sans");
    expect(l).toContain("Liberation Serif");
    expect(l).toContain("Liberation Mono");
    expect(l).toContain("Liberation Sans:style=Bold Italic");
    // font-only context: no builtins mixed in.
    expect(l).not.toContain("cube");
  });

  it("replaces the partially-typed family (from after the opening quote)", () => {
    const doc = 'text(font="Lib';
    const r = complete(doc);
    expect(r).not.toBeNull();
    expect(r!.from).toBe(doc.indexOf('"') + 1); // start of the typed value
    expect(r!.options.some((o) => o.label === "Liberation Sans")).toBe(true);
  });

  it("does not offer fonts once the string is closed", () => {
    const l = labels('text(font="Liberation Sans", size=5)');
    expect(l).not.toContain("Liberation Serif");
  });
});

describe("workspace file completion", () => {
  beforeEach(() => {
    setWorkspaceFiles(["gears.scad", "logo.svg", "part.dxf", "mesh.stl"]);
  });
  afterEach(() => setWorkspaceFiles([]));

  it("offers sibling .scad files inside `include <…>`", () => {
    const doc = "include <";
    const r = complete(doc);
    expect(r).not.toBeNull();
    expect(r!.from).toBe(doc.length); // just after the `<`
    const l = r!.options.map((o) => o.label);
    expect(l).toContain("gears.scad");
    // include/use is source-only: no assets, no builtins.
    expect(l).not.toContain("logo.svg");
    expect(l).not.toContain("cube");
  });

  it("offers .scad files inside `use <…>` and replaces the typed prefix", () => {
    const doc = "use <gea";
    const r = complete(doc);
    expect(r).not.toBeNull();
    expect(r!.from).toBe(doc.indexOf("<") + 1);
    expect(r!.options.map((o) => o.label)).toContain("gears.scad");
  });

  it('offers non-.scad assets inside `import("…")`', () => {
    const doc = 'import("';
    const r = complete(doc);
    expect(r).not.toBeNull();
    expect(r!.from).toBe(doc.length);
    const l = r!.options.map((o) => o.label);
    expect(l).toContain("mesh.stl");
    expect(l).toContain("logo.svg");
    expect(l).toContain("part.dxf");
    // import can't take a .scad, and no builtins in a path string.
    expect(l).not.toContain("gears.scad");
    expect(l).not.toContain("cube");
  });

  it('offers assets inside the named `import(file="…")` form', () => {
    const doc = 'import(file="me';
    const r = complete(doc);
    expect(r).not.toBeNull();
    expect(r!.from).toBe(doc.lastIndexOf('"') + 1);
    expect(r!.options.map((o) => o.label)).toContain("mesh.stl");
  });

  it('offers assets inside `surface("…")`', () => {
    const doc = 'surface("';
    const l = complete(doc)?.options.map((o) => o.label) ?? [];
    expect(l).toContain("mesh.stl");
  });

  it("does not trigger file completion on a non-first string argument", () => {
    const doc = 'import("mesh.stl", layer="';
    const r = complete(doc);
    // Falls through to the ordinary word source (or null), never a file list.
    const l = r?.options.map((o) => o.label) ?? [];
    expect(l).not.toContain("mesh.stl");
  });

  it("offers nothing (but not builtins) when no siblings match", () => {
    setWorkspaceFiles(["logo.svg"]); // no .scad sibling
    const r = complete("include <");
    expect(r).not.toBeNull();
    expect(r!.options).toHaveLength(0);
  });
});
