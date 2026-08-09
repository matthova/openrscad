// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { loadPrefs, savePrefs } from "./prefs";

function enterTauri() {
  (window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ =
    {};
}

afterEach(() => {
  localStorage.clear();
  delete (window as unknown as { __TAURI_INTERNALS__?: unknown })
    .__TAURI_INTERNALS__;
});

describe("prefs system-fonts default", () => {
  it("defaults off in the browser (Local Font Access needs a gesture)", () => {
    expect(loadPrefs().systemFonts).toBe(false);
  });

  it("defaults on in the desktop shell (native, no permission prompt)", () => {
    enterTauri();
    expect(loadPrefs().systemFonts).toBe(true);
  });

  it("respects an explicit stored choice over the host default", () => {
    // Desktop user who turned it off keeps it off.
    enterTauri();
    savePrefs({ systemFonts: false });
    expect(loadPrefs().systemFonts).toBe(false);

    // Browser user who turned it on keeps it on.
    delete (window as unknown as { __TAURI_INTERNALS__?: unknown })
      .__TAURI_INTERNALS__;
    localStorage.clear();
    savePrefs({ systemFonts: true });
    expect(loadPrefs().systemFonts).toBe(true);
  });
});
