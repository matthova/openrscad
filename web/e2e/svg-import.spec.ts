import { test, expect } from "@playwright/test";
import { gotoApp, setEditor, waitForRerender } from "./helpers";

// SVG import parity: the geom importer is unit-tested, but nothing exercised an
// authored `import("x.svg")` through the web UI. These cover the two ways a user
// brings an SVG in (drag-drop, the Project ▾ menu) and that `import()` consumes
// it — a 2D profile that extrudes to a solid.

const STAR_SVG = `<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
  <rect x="10" y="20" width="40" height="30"/>
  <circle cx="70" cy="70" r="15"/>
</svg>`;

async function dropOn(
  page: import("@playwright/test").Page,
  selector: string,
  name: string,
  body: string,
) {
  const dt = await page.evaluateHandle(
    ({ name, body }) => {
      const dt = new DataTransfer();
      // The mime type a browser attaches when dropping a .svg from the OS.
      dt.items.add(new File([body], name, { type: "image/svg+xml" }));
      return dt;
    },
    { name, body },
  );
  await page.locator(selector).dispatchEvent("dragover", { dataTransfer: dt });
  await page.locator(selector).dispatchEvent("drop", { dataTransfer: dt });
}

test("Project ▾ offers Import file…", async ({ page }) => {
  await gotoApp(page);
  await page.getByRole("button", { name: "Project" }).click();
  await expect(page.getByText("Import file…")).toBeVisible();
});

// Dropping onto the editor is the natural target; assert CodeMirror doesn't
// swallow the drop before the app's frame handler sees it.
for (const target of [".app", ".cm-content", ".viewer"]) {
  test(`dropping an .svg on ${target} adds it as a tab`, async ({ page }) => {
    await gotoApp(page);
    await dropOn(page, target, "star.svg", STAR_SVG);
    await expect(page.locator(".tab", { hasText: "star.svg" })).toBeVisible({
      timeout: 5000,
    });
  });
}

test("import()ing a dropped .svg extrudes to a solid", async ({ page }) => {
  await gotoApp(page);
  await dropOn(page, ".app", "star.svg", STAR_SVG);
  await expect(page.locator(".tab", { hasText: "star.svg" })).toBeVisible();
  await waitForRerender(page, () =>
    setEditor(page, 'linear_extrude(height=5) import("star.svg");\n'),
  );
  // The rect (40×30) + circle (r=15) profile, extruded 5mm, is a real volume.
  await expect(page.locator(".status-integrity")).toHaveText("EXACT");
});
