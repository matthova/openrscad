// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { bytesToBase64 } from "./bytes";
import {
  disableSystemFonts,
  enableSystemFonts,
  fontBlobsForSource,
  fontPreview,
  systemFontCompletions,
  systemFontsSupported,
} from "./systemFonts";

// Mock the dynamically-imported Tauri core so the desktop `list_fonts` path can
// be driven without a real Tauri shell. `vi.hoisted` makes the spy available to
// the hoisted `vi.mock` factory.
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

// A fake FontData whose blob() yields fixed bytes (so identical `bytes` model a
// single `.ttc` shared by several faces — which must be sent to the engine once).
function fakeFont(family: string, style: string, ps: string, bytes: number[]) {
  return {
    family,
    style,
    fullName: `${family} ${style}`,
    postscriptName: ps,
    blob: async () => ({
      arrayBuffer: async () => new Uint8Array(bytes).buffer,
    }),
  };
}

const FONTS = [
  fakeFont("TestSans", "Regular", "TestSans", [1, 2, 3]),
  fakeFont("TestSans", "Bold", "TestSans-Bold", [1, 2, 3]), // same file as Regular
  fakeFont("OtherFont", "Regular", "OtherFont", [9, 9]),
];

function installQuery() {
  (
    window as unknown as { queryLocalFonts: () => Promise<unknown> }
  ).queryLocalFonts = async () => FONTS;
}

afterEach(() => {
  disableSystemFonts();
  delete (window as unknown as { queryLocalFonts?: unknown }).queryLocalFonts;
});

describe("systemFonts", () => {
  it("reports support based on the Local Font Access API", () => {
    expect(systemFontsSupported()).toBe(false);
    installQuery();
    expect(systemFontsSupported()).toBe(true);
  });

  it("fails gracefully when unsupported", async () => {
    const res = await enableSystemFonts();
    expect(res.ok).toBe(false);
    expect(systemFontCompletions()).toEqual([]);
  });

  it("lists installed families/styles as completions once enabled", async () => {
    installQuery();
    const res = await enableSystemFonts();
    expect(res).toEqual({ ok: true, families: 2 });
    const labels = systemFontCompletions().map((c) => c.label);
    expect(labels).toContain("TestSans");
    expect(labels).toContain("TestSans:style=Bold");
    expect(labels).toContain("OtherFont");
  });

  it("sends only referenced families' bytes, deduping shared files", async () => {
    installQuery();
    await enableSystemFonts();

    // Both TestSans faces share one file → one blob, not two.
    const forSans = await fontBlobsForSource(
      'text("x", font="TestSans:style=Bold");',
    );
    expect(forSans).toEqual([bytesToBase64(new Uint8Array([1, 2, 3]))]);

    // A different family yields its own file; an unreferenced family, nothing.
    expect(await fontBlobsForSource('text("x", font="OtherFont");')).toEqual([
      bytesToBase64(new Uint8Array([9, 9])),
    ]);
    expect(await fontBlobsForSource("cube(1);")).toEqual([]);
  });

  it("renders a pangram preview in the requested face", () => {
    const el = fontPreview("Liberation Mono:style=Bold Italic");
    expect(el.style.fontFamily).toContain("Liberation Mono");
    expect(el.style.fontFamily).toContain("Courier"); // metric-compatible fallback
    expect(el.style.fontWeight).toBe("700");
    expect(el.style.fontStyle).toBe("italic");
    expect(el.textContent).toContain("The quick brown fox");

    // A plain family (regular) with a system-style name resolves to weight 400.
    const plain = fontPreview("Georgia");
    expect(plain.style.fontFamily).toContain("Georgia");
    expect(plain.style.fontWeight).toBe("400");
    expect(plain.style.fontStyle).toBe("normal");
  });

  it("returns no blobs when disabled", async () => {
    installQuery();
    await enableSystemFonts();
    disableSystemFonts();
    expect(await fontBlobsForSource('text(font="TestSans");')).toEqual([]);
  });
});

// The desktop (Tauri) shell has no Local Font Access API; it fetches the native
// font list over IPC (`list_fonts`) instead, and the engine reads the actual
// font files from disk, so no bytes are transferred from the frontend.
describe("systemFonts on desktop (Tauri)", () => {
  afterEach(() => {
    disableSystemFonts();
    invoke.mockReset();
    delete (window as unknown as { __TAURI_INTERNALS__?: unknown })
      .__TAURI_INTERNALS__;
  });

  function enterTauri() {
    (
      window as unknown as { __TAURI_INTERNALS__: unknown }
    ).__TAURI_INTERNALS__ = {};
  }

  it("reports support inside the desktop shell without queryLocalFonts", () => {
    expect(systemFontsSupported()).toBe(false);
    enterTauri();
    expect(systemFontsSupported()).toBe(true);
  });

  it("lists native fonts as completions and needs no blobs", async () => {
    enterTauri();
    invoke.mockResolvedValue([
      { value: "Helvetica", detail: "Helvetica — Regular" },
      { value: "Helvetica:style=Bold", detail: "Helvetica — Bold" },
      { value: "Menlo", detail: "Menlo — Regular" },
    ]);

    const res = await enableSystemFonts();
    expect(invoke).toHaveBeenCalledWith("list_fonts");
    expect(res).toEqual({ ok: true, families: 2 }); // Helvetica + Menlo

    const labels = systemFontCompletions().map((c) => c.label);
    expect(labels).toEqual(["Helvetica", "Helvetica:style=Bold", "Menlo"]);

    // The native engine reads font files from disk, so the frontend sends none.
    expect(await fontBlobsForSource('text("x", font="Helvetica");')).toEqual(
      [],
    );
  });

  it("fails gracefully when the native command errors", async () => {
    enterTauri();
    invoke.mockRejectedValue(new Error("no fonts"));
    const res = await enableSystemFonts();
    expect(res.ok).toBe(false);
    expect(systemFontCompletions()).toEqual([]);
  });
});
