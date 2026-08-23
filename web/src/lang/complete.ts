// Autocompletion source for OpenSCAD: built-in modules/functions, language
// keywords, and user-defined symbols (modules, functions, assignments, and
// parameters) scraped from the Lezer syntax tree.
import type {
  Completion,
  CompletionContext,
  CompletionResult,
} from "@codemirror/autocomplete";
import { syntaxTree } from "@codemirror/language";
import { BUILTINS } from "./builtins";
import { systemFontCompletions, fontInfo } from "../systemFonts";
import { scadFileCompletions, assetFileCompletions } from "./workspaceFiles";

// The `font=` values offered inside a `text(font="…")` string. Mirrors the
// bundled Liberation family in crates/openrscad-eval/src/text.rs (and its
// `font_completions`): each family on its own (Regular) plus the OpenSCAD
// `Family:style=Style` form for the other styles. Matching is case-insensitive
// in the engine, so the display casing here is cosmetic.
const FONT_COMPLETIONS: Completion[] = (() => {
  const families = ["Liberation Sans", "Liberation Serif", "Liberation Mono"];
  const styles: [suffix: string, label: string][] = [
    ["", "Regular"],
    [":style=Bold", "Bold"],
    [":style=Italic", "Italic"],
    [":style=Bold Italic", "Bold Italic"],
  ];
  const out: Completion[] = [];
  for (const family of families) {
    for (const [suffix, label] of styles) {
      out.push({
        label: family + suffix,
        type: "constant",
        detail: `${family} — ${label}`,
        info: fontInfo,
      });
    }
  }
  return out;
})();

// Language keywords not otherwise surfaced as builtins. `if`/`for`/`let` also
// appear in BUILTINS (with richer signatures), so they're intentionally omitted
// here to avoid duplicate entries.
const KEYWORDS = [
  "module",
  "function",
  "else",
  "each",
  "include",
  "use",
  "true",
  "false",
  "undef",
];

/** Walk the syntax tree and collect user-defined names as completions:
 *  module/function definitions (with their parameter list as detail) plus
 *  assignment targets and module/function parameters. */
function collectUserSymbols(ctx: CompletionContext): Completion[] {
  const out: Completion[] = [];
  const doc = ctx.state.doc;
  const text = (from: number, to: number) => doc.sliceString(from, to);
  const cursor = syntaxTree(ctx.state).cursor();
  do {
    const type = cursor.name;
    if (type === "ModuleDefinition" || type === "FunctionDefinition") {
      const node = cursor.node;
      const nameNode = node.getChild("VariableName");
      if (!nameNode) continue;
      const params = node.getChild("ParamList");
      out.push({
        label: text(nameNode.from, nameNode.to),
        type: "function",
        detail: params ? text(params.from, params.to) : undefined,
      });
    } else if (type === "Assignment" || type === "Parameter") {
      const nameNode = cursor.node.getChild("VariableName");
      if (nameNode) {
        out.push({ label: text(nameNode.from, nameNode.to), type: "variable" });
      }
    }
  } while (cursor.next());
  return out;
}

/** CodeMirror completion source for OpenSCAD. Triggers on identifier/`$var`
 *  prefixes (and explicitly via Ctrl-Space). */
export function openscadCompletion(
  ctx: CompletionContext,
): CompletionResult | null {
  // Inside a `font="…"` string, offer the bundled fonts and nothing else. The
  // match ends at the cursor, so it only fires while the string is still open
  // (`[^"]*` can't cross the closing quote); `from` is placed just after the
  // opening quote so a family name with a space replaces cleanly.
  const fontStr = ctx.matchBefore(/font\s*=\s*"[^"]*/);
  if (fontStr) {
    const from = fontStr.from + fontStr.text.indexOf('"') + 1;
    // Bundled Liberation first, then any installed system fonts the user has
    // granted access to (empty until enabled — see systemFonts.ts). Dedup by
    // label so a system Liberation doesn't double the bundled entries.
    const byLabel = new Map<string, Completion>();
    for (const c of FONT_COMPLETIONS) byLabel.set(c.label, c);
    for (const c of systemFontCompletions()) byLabel.set(c.label, c);
    return { from, options: [...byLabel.values()], validFor: /^[^"]*$/ };
  }

  // Inside an `include <…>` or `use <…>` path, offer sibling `.scad` files.
  // `from` is placed just after the `<` so the bare filename replaces what's
  // typed, and the value can't cross `>` or a newline (mirrors the IncludePath
  // grammar token). Returned even when empty so builtins don't leak into a path.
  const incPath = ctx.matchBefore(/\b(?:include|use)\s*<[^>\n]*/);
  if (incPath) {
    const from = incPath.from + incPath.text.indexOf("<") + 1;
    return { from, options: scadFileCompletions(), validFor: /^[^>\n]*$/ };
  }

  // Inside an `import("…")` / `surface("…")` path string — including the named
  // `file="…"` form — offer sibling asset files. Like `font=`, `from` sits just
  // after the opening quote and the match can't cross the closing quote. The
  // leading `\s*` after `(` only admits the first string argument, so a later
  // string arg (e.g. import's `layer=`) doesn't trigger file completion.
  const filePath = ctx.matchBefore(
    /\b(?:import|surface)\s*\(\s*(?:file\s*=\s*)?"[^"\n]*/,
  );
  if (filePath) {
    const from = filePath.from + filePath.text.indexOf('"') + 1;
    return { from, options: assetFileCompletions(), validFor: /^[^"\n]*$/ };
  }

  const word = ctx.matchBefore(/\$?[\w]*/);
  if (!word || (word.from === word.to && !ctx.explicit)) return null;

  // Dedup by label across sources; later insertions win. Order: keywords, then
  // builtins (override the `if`/`for`/`let` keyword stubs with richer info),
  // then user symbols (override builtins on name collision).
  const byName = new Map<string, Completion>();
  for (const kw of KEYWORDS) byName.set(kw, { label: kw, type: "keyword" });
  for (const b of BUILTINS) {
    byName.set(b.name, {
      label: b.name,
      type: b.name.startsWith("$") ? "variable" : "function",
      detail: b.signature,
      info: b.doc,
    });
  }
  for (const u of collectUserSymbols(ctx)) byName.set(u.label, u);

  return {
    from: word.from,
    options: [...byName.values()],
    validFor: /^\$?[\w]*$/,
  };
}
