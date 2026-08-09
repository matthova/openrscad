import { test, expect } from "@playwright/test";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { gotoApp, setEditor, waitForRerender } from "./helpers";

// Local-file import. A dropped .scad on the pristine default project becomes
// main and renders; a binary mesh (binary STL/3MF) is stored as a byte asset and
// resolves through `import()`; a format the engine can't read surfaces a message.

async function drop(
  page: import("@playwright/test").Page,
  name: string,
  body: string,
) {
  const dt = await page.evaluateHandle(
    ({ name, body }) => {
      const dt = new DataTransfer();
      dt.items.add(new File([body], name, { type: "text/plain" }));
      return dt;
    },
    { name, body },
  );
  await page.locator(".app").dispatchEvent("dragover", { dataTransfer: dt });
  await page.locator(".app").dispatchEvent("drop", { dataTransfer: dt });
}

/** Drop a binary file, reconstructed in the page from its raw bytes. */
async function dropBytes(
  page: import("@playwright/test").Page,
  name: string,
  bytes: Uint8Array,
) {
  const dt = await page.evaluateHandle(
    ({ name, bytes }) => {
      const dt = new DataTransfer();
      dt.items.add(
        new File([new Uint8Array(bytes)], name, {
          type: "application/octet-stream",
        }),
      );
      return dt;
    },
    { name, bytes: Array.from(bytes) },
  );
  await page.locator(".app").dispatchEvent("dragover", { dataTransfer: dt });
  await page.locator(".app").dispatchEvent("drop", { dataTransfer: dt });
}

test("dropping a .scad loads and renders it", async ({ page }) => {
  await gotoApp(page);
  await waitForRerender(page, () => drop(page, "widget.scad", "sphere(12);\n"));
  await expect(page.locator(".tab", { hasText: "widget.scad" })).toBeVisible();
  await expect(page.locator(".editor")).toContainText("sphere(12)");
  await expect(page.locator(".status-integrity")).toHaveText("EXACT");
});

test("importing a binary STL renders through import()", async ({ page }) => {
  // A 12-triangle binary STL cube (the geom corpus fixture).
  const stl = readFileSync(
    fileURLToPath(new URL("../../corpus/geom/cube.stl", import.meta.url)),
  );
  await gotoApp(page);
  await dropBytes(page, "cube.stl", new Uint8Array(stl));
  // The asset lands as a read-only tab with a placeholder body.
  await expect(page.locator(".tab", { hasText: "cube.stl" })).toBeVisible();
  // Point the main file at it and confirm it renders real geometry.
  await page.locator(".tab", { hasText: "main.scad" }).click();
  await waitForRerender(page, () => setEditor(page, 'import("cube.stl");'));
  await expect(page.locator(".status-integrity")).toHaveText("EXACT");
  await expect(page.locator(".status-meta")).toContainText("vol");
});

test("the Import STL example renders its bundled mesh", async ({ page }) => {
  await gotoApp(page);
  page.on("dialog", (d) => d.accept()); // loadExample confirms before replacing
  await waitForRerender(page, () =>
    page.locator(".examples-select").selectOption({ label: "Import STL" }),
  );
  await expect(page.locator(".tab", { hasText: "tetra.stl" })).toBeVisible();
  await expect(page.locator(".status-integrity")).toHaveText("EXACT");
});

test("dropping an unsupported file surfaces a message", async ({ page }) => {
  await gotoApp(page);
  await drop(page, "logo.png", "not-really-a-png");
  await expect(page.locator(".update-banner.error")).toContainText(
    "can't import",
    { ignoreCase: true },
  );
});
