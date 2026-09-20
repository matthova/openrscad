import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

// The standalone brand page (brand.html → served at /openrscad/brand). It is
// reachable by right-clicking the nav logo and from the footer, and offers the
// logo mark and lockup as SVG downloads for light and dark backgrounds.

const ASSETS = [
  "logos/openrscad-mark-light.svg",
  "logos/openrscad-mark-dark.svg",
  "logos/openrscad-lockup-light.svg",
  "logos/openrscad-lockup-dark.svg",
] as const;

test("right-clicking the nav logo opens the brand page", async ({ page }) => {
  await page.goto("/");
  await page.locator(".mk-brand").click({ button: "right" });
  await page.waitForURL("**/brand");
  expect(new URL(page.url()).pathname).toBe("/brand");
  await expect(
    page.getByRole("heading", { level: 1, name: /brand assets/i }),
  ).toBeVisible();
});

test("the footer brand link opens the brand page", async ({ page }) => {
  await page.goto("/");
  await page
    .locator(".mk-footer-links")
    .getByRole("link", { name: "Brand" })
    .click();
  await page.waitForURL("**/brand");
  expect(new URL(page.url()).pathname).toBe("/brand");
});

test("brand page offers all four logo downloads and the files resolve", async ({
  page,
}) => {
  await page.goto("/brand");

  // Four download buttons, each pointing at its SVG with a download attribute.
  const downloads = page.locator("a.mk-btn[download]");
  await expect(downloads).toHaveCount(ASSETS.length);

  for (const href of ASSETS) {
    const link = page.locator(`a.mk-btn[href="${href}"]`);
    await expect(link).toHaveAttribute("download", "");

    // The asset actually resolves and is an SVG.
    const res = await page.request.get(`/${href}`);
    expect(res.status()).toBe(200);
    expect(await res.text()).toContain("<svg");
  }

  // Both preview tiles for each group render their image (4 total).
  await expect(page.locator(".mk-asset-preview img")).toHaveCount(4);
});

test("brand page theme toggle flips and persists", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  await page.goto("/brand");
  const html = page.locator("html");
  await expect(html).toHaveAttribute("data-theme", "dark");

  await page.getByRole("button", { name: /switch to light theme/i }).click();
  await expect(html).toHaveAttribute("data-theme", "light");
  expect(await page.evaluate(() => localStorage.getItem("orscad-theme"))).toBe(
    "light",
  );

  await page.reload();
  await expect(html).toHaveAttribute("data-theme", "light");
});

for (const scheme of ["dark", "light"] as const) {
  test(`brand page has no axe violations (${scheme})`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await page.goto("/brand");
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
}
