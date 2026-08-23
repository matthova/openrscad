// The other files in the current workspace, offered as path completions inside
// `include <…>` / `use <…>` and the `file="…"` argument of import()/surface().
//
// Mirrors the systemFonts.ts pattern: App feeds the current sibling-file list
// here whenever the workspace changes (see setWorkspaceFiles), and the completion
// source in complete.ts reads it at trigger time. Names are bare — the browser's
// MapResolver (crates/openrscad-wasm) resolves siblings by name, so a bare file
// name is exactly what belongs inside the path.
import type { Completion } from "@codemirror/autocomplete";

/** Sibling file names in the current workspace (typically every file but the one
 *  being edited). Empty until App calls setWorkspaceFiles. */
let names: string[] = [];

/** Replace the workspace file list offered in path completions. Callers pass the
 *  sibling names — everything but the file being edited — so a file never offers
 *  to include itself. */
export function setWorkspaceFiles(fileNames: string[]): void {
  names = fileNames;
}

function fileCompletions(keep: (name: string) => boolean): Completion[] {
  return names
    .filter(keep)
    .map((label) => ({ label, type: "text", detail: "workspace file" }));
}

/** Sibling files `include`/`use` can pull in: OpenSCAD source only. */
export function scadFileCompletions(): Completion[] {
  return fileCompletions((n) => n.toLowerCase().endsWith(".scad"));
}

/** Sibling files `import()`/`surface()` can read: any asset that isn't a `.scad`
 *  (STL/OFF/3MF/SVG/DXF/AMF/OBJ, PNG/DAT heightmaps, …). */
export function assetFileCompletions(): Completion[] {
  return fileCompletions((n) => !n.toLowerCase().endsWith(".scad"));
}
