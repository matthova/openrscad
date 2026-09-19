// Shared chrome for the standalone marketing pages (index.html + brand.html):
// the light/dark toggle, the mobile nav menu, and the right-click-the-logo
// shortcut to the brand page. Kept framework-free so a page can pull in only
// the pieces it renders. about.ts owns the rest of the landing page; brand.ts
// is otherwise tiny.

// ── Theme (manual toggle + OS follow) ───────────────────────────────────────
// A pre-paint script in the page <head> already set data-theme from a saved
// choice or the OS. Here we keep the toggle button in sync, persist a manual
// choice, and — only while there's no manual choice — follow later OS changes.
const THEME_KEY = "orscad-theme";

function currentTheme(): "dark" | "light" {
  return document.documentElement.dataset.theme === "light" ? "light" : "dark";
}

function applyTheme(t: "dark" | "light") {
  document.documentElement.dataset.theme = t;
  const btn = document.getElementById("theme-toggle");
  const label = btn?.querySelector(".mk-theme-label");
  if (label) label.textContent = t === "dark" ? "Dark" : "Light";
  btn?.setAttribute(
    "aria-label",
    t === "dark" ? "Switch to light theme" : "Switch to dark theme",
  );
}

export function wireTheme() {
  applyTheme(currentTheme());
  document.getElementById("theme-toggle")?.addEventListener("click", () => {
    const next = currentTheme() === "dark" ? "light" : "dark";
    try {
      localStorage.setItem(THEME_KEY, next);
    } catch {
      /* private mode — the choice just won't persist */
    }
    applyTheme(next);
  });
  // Follow the OS only until the visitor picks a side themselves.
  window
    .matchMedia("(prefers-color-scheme: dark)")
    .addEventListener("change", (e) => {
      let saved: string | null = null;
      try {
        saved = localStorage.getItem(THEME_KEY);
      } catch {
        /* ignore */
      }
      if (!saved) applyTheme(e.matches ? "dark" : "light");
    });
}

// ── Mobile nav menu ─────────────────────────────────────────────────────────
export function wireMenu() {
  const box = document.querySelector(".mk-nav-box");
  const toggle = document.getElementById("menu-toggle");
  if (!box || !toggle) return;
  const set = (open: boolean) => {
    box.setAttribute("data-menu", open ? "open" : "closed");
    toggle.setAttribute("aria-expanded", String(open));
    document.body.style.overflow = open ? "hidden" : "";
  };
  toggle.addEventListener("click", () =>
    set(box.getAttribute("data-menu") !== "open"),
  );
  // Any menu link closes it; Escape closes it.
  box
    .querySelector(".mk-mobile-menu")
    ?.querySelectorAll("a")
    .forEach((a) => a.addEventListener("click", () => set(false)));
  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape") set(false);
  });
  // Growing to the desktop layout retires the burger/overlay, so close an open
  // menu on the way up — otherwise it lingers over the desktop nav and leaves
  // body scroll locked. 881px mirrors the CSS breakpoint (max-width: 880px).
  const desktop = window.matchMedia("(min-width: 881px)");
  desktop.addEventListener("change", (e) => {
    if (e.matches) set(false);
  });
}

// ── Logo right-click → brand page ────────────────────────────────────────────
// Right-clicking the logo is the conventional way into a brand/press-assets
// page. We swallow the default context menu on the logo only and send visitors
// to the brand page (also reachable from the footer link, for keyboard users).
// `href` is resolved relative to the current document, so "brand" works from
// both the site root and the brand page itself.
export function wireLogoContextMenu(href = "brand") {
  document.querySelector(".mk-brand")?.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    window.location.href = href;
  });
}
