// System-font access for `text(font="…")` — enumerating the user's installed
// fonts for autocomplete and (in the browser) feeding their bytes to the engine.
//
// Two paths, chosen by host:
//
//   • Browser: the Local Font Access API (`queryLocalFonts`). Chromium-only
//     (Chrome/Edge/Opera — not Firefox/Safari) and gated behind a user-permission
//     prompt. Rendering happens in a wasm worker with no filesystem, so the engine
//     can't read font files itself — we hand it the raw bytes. To keep transfers
//     small we only read the *bytes* of families a model actually references (the
//     metadata list, used for autocomplete, is cheap and gathered up front). See
//     `fontBlobsForSource`.
//
//   • Desktop (Tauri): the native engine already reads OS font files from disk
//     when it renders, so it needs no bytes from us — we only fetch the *names*
//     for autocomplete, via the `list_fonts` IPC command (the desktop webview,
//     WKWebView on macOS, doesn't implement the Local Font Access API).
//
// Either way it degrades gracefully: when unavailable or denied, the playground
// keeps the bundled Liberation family (see crates/openrscad-eval/src/text.rs).
import type { Completion } from "@codemirror/autocomplete";
import { bytesToBase64 } from "./bytes";

/** One installed font face, per the Local Font Access API's `FontData`. */
interface FontData {
  family: string;
  fullName: string;
  postscriptName: string;
  style: string;
  blob(): Promise<Blob>;
}

declare global {
  interface Window {
    queryLocalFonts?: () => Promise<FontData[]>;
  }
}

/** Faces of the last successful query, grouped by lowercased family name. Null
 *  until the user enables (and grants) system fonts. Browser path only. */
let byFamily: Map<string, FontData[]> | null = null;

/** Autocomplete entries from the native `list_fonts` command. Null until the
 *  user enables system fonts in the desktop shell; distinct from `byFamily`
 *  because the native engine reads font files from disk itself, so we fetch only
 *  the names (for autocomplete), never the bytes. */
let desktopFonts: Completion[] | null = null;

/** True inside the Tauri desktop shell, where system fonts come from the native
 *  engine (over IPC) rather than the browser's Local Font Access API. Mirrors
 *  `isTauri` in desktopEngine.ts; inlined to keep this module free of the
 *  desktop-engine import chain. */
function isDesktop(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** One entry from the native `list_fonts` command (serde camelCase). */
interface FontEntry {
  value: string;
  detail: string;
}

/** base64 of each face's file bytes, keyed by PostScript name. Cached because a
 *  font file never changes within a session; `null` marks a face whose bytes we
 *  failed to read (so we don't retry it every render). */
const b64ByFace = new Map<string, string | null>();

/** Whether this host can enumerate system fonts: the desktop shell (native), or
 *  a browser exposing the Local Font Access API. */
export function systemFontsSupported(): boolean {
  return (
    isDesktop() ||
    (typeof window !== "undefined" &&
      typeof window.queryLocalFonts === "function")
  );
}

/** Fetch the native font list (desktop only) and build autocomplete entries.
 *  No permission prompt or byte transfer — the native engine reads font files
 *  from disk when it renders. */
async function enableDesktopFonts(): Promise<
  { ok: true; families: number } | { ok: false; error: string }
> {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const entries = await invoke<FontEntry[]>("list_fonts");
    desktopFonts = entries.map((e) => ({
      label: e.value,
      type: "constant",
      detail: e.detail,
      info: fontInfo,
    }));
    // Distinct families = labels before any `:style=` suffix.
    const families = new Set(
      entries.map((e) => e.value.split(":")[0].toLowerCase()),
    );
    return { ok: true, families: families.size };
  } catch (err) {
    return { ok: false, error: String(err) };
  }
}

/** Enable system fonts: desktop fetches the native list; the browser prompts for
 *  the Local Font Access permission and enumerates installed fonts. In the
 *  browser this must be called from a user gesture (the permission prompt
 *  requires it). Returns the number of families found, or an error string on
 *  denial/failure. */
export async function enableSystemFonts(): Promise<
  { ok: true; families: number } | { ok: false; error: string }
> {
  if (isDesktop()) return enableDesktopFonts();
  if (!systemFontsSupported()) {
    return {
      ok: false,
      error: "This browser doesn't support system fonts (Chromium only).",
    };
  }
  try {
    const fonts = await window.queryLocalFonts!();
    const map = new Map<string, FontData[]>();
    for (const f of fonts) {
      const key = f.family.toLowerCase();
      const list = map.get(key);
      if (list) list.push(f);
      else map.set(key, [f]);
    }
    byFamily = map;
    return { ok: true, families: map.size };
  } catch (err) {
    // Permission denied, dismissed, or otherwise unavailable.
    return { ok: false, error: String(err) };
  }
}

/** Forget the queried fonts (turning system fonts back off). */
export function disableSystemFonts(): void {
  byFamily = null;
  desktopFonts = null;
}

/** The coarse OpenSCAD `:style=` bucket for a `FontData.style` string, matching
 *  the engine's own bucketing (see `style_label` in text.rs) so a completion
 *  round-trips to the same face. */
function styleBucket(
  style: string,
): "Regular" | "Bold" | "Italic" | "Bold Italic" {
  const s = style.toLowerCase();
  const bold = s.includes("bold");
  const italic = s.includes("italic") || s.includes("oblique");
  if (bold && italic) return "Bold Italic";
  if (bold) return "Bold";
  if (italic) return "Italic";
  return "Regular";
}

/** Metric-compatible CSS fallbacks for the bundled Liberation families, so their
 *  autocomplete preview looks right even on machines without Liberation installed
 *  (Liberation Sans↔Arial, Serif↔Times, Mono↔Courier). System fonts are already
 *  installed, so their own name resolves and the fallback is just a safety net. */
function cssFallback(family: string): string {
  const f = family.toLowerCase();
  if (f === "liberation sans") return "Arial, Helvetica, sans-serif";
  if (f === "liberation serif") return "'Times New Roman', Times, serif";
  if (f === "liberation mono") return "'Courier New', Courier, monospace";
  if (f.includes("serif")) return "serif";
  if (f.includes("mono")) return "monospace";
  return "sans-serif";
}

/** A DOM preview of a `font=` value, rendered in that actual font — used as the
 *  CodeMirror completion `info` so a pangram sample shows next to the highlighted
 *  option as you scroll. `value` is the `Family` / `Family:style=Style` string. */
export function fontPreview(value: string): HTMLElement {
  const colon = value.indexOf(":");
  const family = (colon === -1 ? value : value.slice(0, colon)).trim();
  const style = styleBucket(value.slice(colon + 1));

  const el = document.createElement("div");
  el.className = "cm-font-preview";
  el.style.fontFamily = `"${family}", ${cssFallback(family)}`;
  el.style.fontWeight = style.includes("Bold") ? "700" : "400";
  el.style.fontStyle = style.includes("Italic") ? "italic" : "normal";

  const sample = document.createElement("div");
  sample.className = "cm-font-preview-sample";
  sample.textContent = "The quick brown fox jumps over the lazy dog";
  const digits = document.createElement("div");
  digits.className = "cm-font-preview-digits";
  digits.textContent = "0123456789 !?&@#";
  el.append(sample, digits);
  return el;
}

/** CodeMirror `info` callback: preview a font completion in its own typeface. */
export function fontInfo(completion: Completion): HTMLElement {
  return fontPreview(completion.label);
}

/** Autocomplete entries for every installed family/style, in the OpenSCAD
 *  `Family` / `Family:style=Style` form. Empty until system fonts are enabled. */
export function systemFontCompletions(): Completion[] {
  // Desktop: the native `list_fonts` command already returns ready-made entries.
  if (desktopFonts) return desktopFonts;
  if (!byFamily) return [];
  const seen = new Set<string>();
  const out: Completion[] = [];
  for (const faces of byFamily.values()) {
    for (const face of faces) {
      const style = styleBucket(face.style);
      const key = `${face.family.toLowerCase()} ${style}`;
      if (seen.has(key)) continue;
      seen.add(key);
      const label =
        style === "Regular" ? face.family : `${face.family}:style=${style}`;
      out.push({
        label,
        type: "constant",
        detail: `${face.family} — ${style}`,
        info: fontInfo,
      });
    }
  }
  return out;
}

/** The distinct font families referenced by `font="…"` in `source`. A light
 *  regex (not a full parse) — over-matching only costs an extra font load. */
function referencedFamilies(source: string): string[] {
  const families = new Set<string>();
  const re = /font\s*=\s*"([^"]*)"/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(source)) !== null) {
    const family = m[1].split(":")[0].trim();
    if (family) families.add(family.toLowerCase());
  }
  return [...families];
}

/** Read and cache a face's file bytes as base64 (or `null` if unreadable). */
async function faceBytes(face: FontData): Promise<string | null> {
  const cached = b64ByFace.get(face.postscriptName);
  if (cached !== undefined) return cached;
  try {
    const buf = new Uint8Array(await (await face.blob()).arrayBuffer());
    const b64 = bytesToBase64(buf);
    b64ByFace.set(face.postscriptName, b64);
    return b64;
  } catch {
    b64ByFace.set(face.postscriptName, null);
    return null;
  }
}

/** Base64 font files for the system families `source` references, ready to hand
 *  to the engine's `font_blobs` channel. Only referenced families are read, and
 *  duplicate files (e.g. every style of one `.ttc`) are sent once. Empty when
 *  system fonts are disabled, and always on desktop (the native engine reads
 *  font files from disk itself — see the module header). Sent every render
 *  because the render worker may be respawned for cancellation; the engine
 *  dedupes reloads by content. */
export async function fontBlobsForSource(source: string): Promise<string[]> {
  if (!byFamily) return [];
  const out: string[] = [];
  const sent = new Set<string>();
  for (const family of referencedFamilies(source)) {
    for (const face of byFamily.get(family) ?? []) {
      const b64 = await faceBytes(face);
      if (b64 && !sent.has(b64)) {
        sent.add(b64);
        out.push(b64);
      }
    }
  }
  return out;
}
