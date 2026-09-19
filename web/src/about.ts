// Standalone entry for the marketing page (index.html, the site root). No React
// — it enhances an otherwise-static page:
// (1) upgrades the hero's primary download button to the visitor's OS,
// (2) drives the light/dark toggle (persisted) + follows the OS otherwise,
// (3) runs the mobile nav menu,
// (4) powers the live hero mini-playground (model + code + sliders), and
// (5) renders the render-shootout chart/table and animates the bars in on scroll.
//
// Everything degrades: the four per-OS cards and the "other options" link are
// plain <a> tags that work without any of this, the primary button ships
// pointing at the releases page, and the hero copy/CTAs stand on their own if the
// widget never hydrates.
import "./about.css";
import { ASSETS, DL, detectOs, isAppleSilicon } from "./downloads";

type Target = {
  /** Filename of the stable release alias, or null for "no desktop build". */
  asset: string | null;
  /** Button label, e.g. "Download for macOS". */
  label: string;
  /** Sub-line under the button. */
  note: string;
};

const MAC_ARM: Target = {
  asset: ASSETS.macArm,
  label: "Download for macOS",
  note: "Apple Silicon · .dmg — Intel, Windows and Linux below. Free, no account.",
};
const MAC_INTEL: Target = {
  asset: ASSETS.macIntel,
  label: "Download for macOS",
  note: "Intel · .dmg — Apple Silicon and other builds below. Free, no account.",
};
const WINDOWS: Target = {
  asset: ASSETS.windows,
  label: "Download for Windows",
  note: "x64 installer — macOS and Linux below. Free, no account.",
};
const LINUX: Target = {
  asset: ASSETS.linux,
  label: "Download for Linux",
  note: "x86_64 · AppImage — .deb / .rpm and other builds below.",
};
// Phones/tablets and anything we can't place: there's no desktop build to push,
// so send them to the browser playground instead.
const OTHER: Target = {
  asset: null,
  label: "Open the playground",
  note: "The desktop app is available for macOS, Windows, and Linux.",
};

async function pickTarget(): Promise<Target> {
  switch (detectOs()) {
    case "mac":
      return (await isAppleSilicon()) ? MAC_ARM : MAC_INTEL;
    case "windows":
      return WINDOWS;
    case "linux":
      return LINUX;
    default:
      return OTHER;
  }
}

async function wirePrimaryDownload() {
  const btn = document.getElementById(
    "primary-download",
  ) as HTMLAnchorElement | null;
  const note = document.getElementById("primary-note");
  if (!btn) return;

  const target = await pickTarget();
  if (!target.asset) {
    // No desktop build for this device (phone/tablet). The hero already has a
    // ghost "Open the playground" button, so upgrading this one would show that
    // CTA twice — hide it instead and let the note explain desktop availability.
    btn.hidden = true;
    if (note) note.textContent = target.note;
    return;
  }
  btn.textContent = target.label;
  btn.href = `${DL}/${target.asset}`;
  if (note) note.textContent = target.note;

  // Highlight the matching card in the OS grid so the autodetected choice and
  // the full list agree at a glance.
  const os = detectOs();
  const cardOs =
    os === "mac"
      ? target === MAC_INTEL
        ? "mac-intel"
        : "mac-arm"
      : os === "windows"
        ? "windows"
        : os === "linux"
          ? "linux"
          : null;
  if (cardOs) {
    document
      .querySelector(`.mk-os-card[data-os="${cardOs}"]`)
      ?.classList.add("mk-os-card--suggested");
  }
}

// ── Theme (manual toggle + OS follow) ───────────────────────────────────────
// A pre-paint script in index.html already set data-theme from a saved choice or
// the OS. Here we keep the toggle button in sync, persist a manual choice, and —
// only while there's no manual choice — follow later OS changes.
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

function wireTheme() {
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
function wireMenu() {
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
}

// ── Hero mini-playground ────────────────────────────────────────────────────
const M_SHADES = ["var(--model)", "var(--model-2)", "var(--model-3)"];

function span(cls: string, text: string): HTMLSpanElement {
  const s = document.createElement("span");
  if (cls) s.className = cls;
  s.textContent = text;
  return s;
}

/** One box face, positioned/rotated in 3D. */
function face(w: number, h: number, bg: string, tf: string, radius: number) {
  const d = document.createElement("div");
  Object.assign(d.style, {
    position: "absolute",
    width: `${w}px`,
    height: `${h}px`,
    left: `${-w / 2}px`,
    top: `${-h / 2}px`,
    background: bg,
    borderRadius: `${radius}px`,
    transform: tf,
    border: "1px solid rgba(0,0,0,.12)",
  } satisfies Partial<CSSStyleDeclaration>);
  return d;
}

/** A rounded box: 4 sides + top + bottom, six faces, in 3D. */
function cube(sz: number, bgs: string[], dz: number, s: number, r: number) {
  const half = s / 2;
  const hh = sz / 2;
  const rr = Math.min(r, sz / 2);
  const wrap = document.createElement("div");
  Object.assign(wrap.style, {
    position: "absolute",
    transformStyle: "preserve-3d",
    transform: `translateY(${dz}px)`,
  } satisfies Partial<CSSStyleDeclaration>);
  const faces: Array<[number, number, string, string, number]> = [
    [s, sz, bgs[0], `translateZ(${half}px)`, rr],
    [s, sz, bgs[1], `rotateY(180deg) translateZ(${half}px)`, rr],
    [s, sz, bgs[1], `rotateY(90deg) translateZ(${half}px)`, rr],
    [s, sz, bgs[0], `rotateY(-90deg) translateZ(${half}px)`, rr],
    [s, s, bgs[2], `rotateX(90deg) translateZ(${hh}px)`, r],
    [s, s, bgs[1], `rotateX(-90deg) translateZ(${hh}px)`, r],
  ];
  for (const [w, h, bg, tf, rad] of faces)
    wrap.appendChild(face(w, h, bg, tf, rad));
  return wrap;
}

/** The spinning rounded box (+ optional floating lid) for the hero stage. */
function heroModel(sizeU: number, radiusU: number, lid: boolean, px: number) {
  const s = sizeU * px;
  const half = s / 2;
  const r = Math.max(2, radiusU * px * 0.9);
  const lidH = Math.max(6, 4 * px);
  const gap = 1 * px;
  const lidLift = lid ? half + gap + radiusU * px + lidH / 2 : 0;
  const total = s + (lid ? lidLift : 0);

  const kids = [cube(s, M_SHADES, lid ? lidLift * 0.35 : 0, s, r)];
  if (lid) {
    kids.push(
      cube(
        lidH,
        ["var(--model-2)", "var(--model-3)", "var(--model)"],
        -(lidLift * 0.65),
        s,
        r,
      ),
    );
  }

  const outer = document.createElement("div");
  Object.assign(outer.style, {
    width: `${total}px`,
    height: `${total}px`,
    position: "relative",
    perspective: "900px",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
  } satisfies Partial<CSSStyleDeclaration>);
  const spinner = document.createElement("div");
  Object.assign(spinner.style, {
    position: "absolute",
    transformStyle: "preserve-3d",
    animation: "mk-spin 22s linear infinite",
  } satisfies Partial<CSSStyleDeclaration>);
  for (const k of kids) spinner.appendChild(k);
  outer.appendChild(spinner);
  return outer;
}

/** The syntax-coloured code listing that mirrors the current slider values. */
function heroCode(
  size: number,
  radius: number,
  lid: boolean,
): DocumentFragment {
  const frag = document.createDocumentFragment();
  const line = (...parts: Array<Node | string>) => {
    for (const p of parts)
      frag.appendChild(typeof p === "string" ? document.createTextNode(p) : p);
    frag.appendChild(document.createTextNode("\n"));
  };
  line(span("cmt", "// drag the sliders — the model re-renders live"));
  line(span("kw", "use"), " <helpers.scad>");
  line(
    span("var", "size"),
    " = ",
    span("num", String(size)),
    ";     ",
    span("cmt", "// [10:60]"),
  );
  line(
    span("var", "radius"),
    " = ",
    span("num", String(radius)),
    ";    ",
    span("cmt", "// [1:12]"),
  );
  line(span("var", "lid"), " = ", span("kw", lid ? "true" : "false"), ";");
  line(span("fn", "rounded_box"), "([size, size, size], radius);");
  line(
    span("ctl", "if"),
    " (lid) ",
    span("fn", "translate"),
    "([",
    span("num", "0"),
    ", ",
    span("num", "0"),
    ", size/",
    span("num", "2"),
    " + radius])",
  );
  line(
    "  ",
    span("fn", "rounded_box"),
    "([size, size, ",
    span("num", "4"),
    "], radius);",
  );
  return frag;
}

function wireHero() {
  const stage = document.getElementById("hero-model");
  const codeEl = document.getElementById("hero-code");
  const meta = document.getElementById("hero-meta");
  const dims = document.getElementById("hero-dims");
  const sizeIn = document.getElementById(
    "hero-size",
  ) as HTMLInputElement | null;
  const radiusIn = document.getElementById(
    "hero-radius",
  ) as HTMLInputElement | null;
  const lidBtn = document.getElementById("hero-lid");
  const sizeVal = document.getElementById("hero-size-val");
  const radiusVal = document.getElementById("hero-radius-val");
  if (!stage || !sizeIn || !radiusIn || !lidBtn) return;

  const state = { size: 30, radius: 4, lid: true };

  const render = () => {
    const { size, radius, lid } = state;
    stage.replaceChildren(heroModel(size, radius, lid, 3.3));
    if (codeEl) codeEl.replaceChildren(heroCode(size, radius, lid));
    const tris = Math.round(size * 86.7 + radius * 220 + (lid ? 2300 : 0));
    const renderMs = Math.round(6 + size * 0.25 + radius * 1.2);
    if (meta)
      meta.textContent = `${tris.toLocaleString()} triangles · ${renderMs} ms · exact`;
    const z = lid ? size + 1 + radius + 4 : size;
    const d = (v: number) => (v - 0.02).toFixed(2);
    if (dims) dims.textContent = `${d(size)} × ${d(size)} × ${d(z)} mm`;
    if (sizeVal) sizeVal.textContent = String(size);
    if (radiusVal) radiusVal.textContent = String(radius);
    sizeIn.style.setProperty("--fill", `${((size - 10) / 50) * 100}%`);
    radiusIn.style.setProperty("--fill", `${((radius - 1) / 11) * 100}%`);
    lidBtn.setAttribute("aria-checked", String(lid));
  };

  sizeIn.addEventListener("input", () => {
    state.size = Number(sizeIn.value);
    render();
  });
  radiusIn.addEventListener("input", () => {
    state.radius = Number(radiusIn.value);
    render();
  });
  lidBtn.addEventListener("click", () => {
    state.lid = !state.lid;
    render();
  });
  render();
}

// ── Render shootout ─────────────────────────────────────────────────────────
// Committed benchmark data: best of 3 runs, full-process wall-clock (ms), on an
// Apple M4 Max vs OpenSCAD 2024.12 (`cargo run -p xtask -- bench`). Ordered by
// CGAL speed-up, descending, so the chart reads big-win → small-win.
type Row = {
  model: string;
  note: string;
  openrscad: number;
  cgal: number;
  mfld: number;
};
const SHOOTOUT: Row[] = [
  {
    model: "Boolean grid",
    note: "spheres + cylinders diffed from a slab",
    openrscad: 53,
    cgal: 19338,
    mfld: 165,
  },
  {
    model: "Gears",
    note: "linear + rotate extrude",
    openrscad: 12,
    cgal: 1877,
    mfld: 59,
  },
  {
    model: "Rounded",
    note: "minkowski + hull",
    openrscad: 21,
    cgal: 334,
    mfld: 54,
  },
  {
    model: "Eval-bound",
    note: "heavy Collatz computation",
    openrscad: 102,
    cgal: 510,
    mfld: 501,
  },
  {
    model: "Lamp shade",
    note: "extrudes + booleans",
    openrscad: 37,
    cgal: 176,
    mfld: 173,
  },
];

/** ms → a compact human string: sub-second in ms, else seconds. */
function fmtTime(ms: number): string {
  return ms < 1000
    ? `${Math.round(ms)} ms`
    : `${(ms / 1000).toFixed(ms < 10000 ? 1 : 0)} s`;
}
/** A speed-up multiplier, e.g. 4.8 → "4.8×", 365 → "365×". */
function fmtX(v: number): string {
  return `${v >= 100 ? Math.round(v) : v.toFixed(1)}×`;
}

// Log TIME axis, 10 ms → 30 s. Render times span ~12 ms to ~19 s, so a linear
// axis would bury every fast bar under the slow one; log keeps all three
// engines readable. Bar length + gridlines share this one scale.
const AXIS_MIN_LOG = 1; // log10(10 ms)
const AXIS_MAX_LOG = Math.log10(30000); // 30 s, headroom past the 19 s worst case
const frac = (ms: number) =>
  Math.max(
    0,
    Math.min(
      1,
      (Math.log10(ms) - AXIS_MIN_LOG) / (AXIS_MAX_LOG - AXIS_MIN_LOG),
    ),
  );

const el = (tag: string, cls?: string, text?: string) => {
  const n = document.createElement(tag);
  if (cls) n.className = cls;
  if (text != null) n.textContent = text;
  return n;
};

// The three series, in legend / stacking order (OpenRSCAD on bottom, highlighted).
// Each row draws a bar per engine; colour identity is backed up by the direct
// time label at the bar's end.
const SERIES = [
  {
    key: "cgal",
    cls: "cgal",
    label: "OpenSCAD · CGAL",
  },
  {
    key: "mfld",
    cls: "mfld",
    label: "OpenSCAD · Manifold",
  },
  {
    key: "openrscad",
    cls: "openrscad",
    label: "OpenRSCAD",
  },
] as const;

/** Grouped horizontal bars per model — one bar per engine, log time axis. Bars
 *  start collapsed and grow once `.in-view` is added (see chart animation). */
function renderChart(host: HTMLElement) {
  const chart = el("div", "mk-chart-inner");

  // Legend (three series → always present; also direct-labeled on each bar).
  const legend = el("div", "mk-legend");
  for (const s of SERIES) {
    const item = el("span", "mk-legend-item");
    item.append(
      el("span", `mk-swatch mk-swatch--${s.cls}`),
      el("span", undefined, s.label),
    );
    legend.append(item);
  }
  chart.append(legend);

  // Gridlines at 10 ms, 100 ms, 1 s, 10 s.
  const plot = el("div", "mk-plot");
  const grid = el("div", "mk-grid");
  for (const [ms, tick] of [
    [10, "10 ms"],
    [100, "100 ms"],
    [1000, "1 s"],
    [10000, "10 s"],
  ] as const) {
    const line = el("div", "mk-gridline");
    line.style.left = `${frac(ms) * 100}%`;
    line.append(el("span", "mk-gridtick", tick));
    grid.append(line);
  }
  plot.append(grid);

  for (const r of SHOOTOUT) {
    const row = el("div", "mk-row");
    const label = el("div", "mk-row-label");
    label.append(
      el("span", "mk-row-name", r.model),
      el("span", "mk-row-note", r.note),
    );
    row.append(label);

    const bars = el("div", "mk-bars");
    for (const s of SERIES) {
      const ms = r[s.key];
      const track = el("div", `mk-track mk-track--${s.cls}`);
      // Target width lives in a CSS var so the bar (width) and its label (left)
      // grow together from 0 when the plot scrolls into view.
      track.style.setProperty("--w", `${frac(ms) * 100}%`);
      const bar = el("div", `mk-bar mk-bar--${s.cls}`);
      const value = el("span", "mk-bar-value", fmtTime(ms));
      track.append(bar, value);
      bars.append(track);
    }
    row.append(bars);
    plot.append(row);
  }
  chart.append(plot);
  host.replaceChildren(chart);

  // Grow the bars in when the chart first scrolls into view (once).
  const reveal = () => plot.classList.add("in-view");
  const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (reduce || !("IntersectionObserver" in window)) {
    reveal();
    return;
  }
  const io = new IntersectionObserver(
    (entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        reveal();
        io.disconnect();
      }
    },
    { threshold: 0.25 },
  );
  io.observe(plot);
}

/** The exact numbers — credibility, and the accessible view of the chart. */
function renderTable(host: HTMLElement) {
  const table = el("table", "mk-table");
  const thead = el("thead");
  const htr = el("tr");
  for (const [h, cls] of [
    ["Model", ""],
    ["OpenRSCAD", "num"],
    ["OpenSCAD CGAL", "num"],
    ["OpenSCAD Manifold", "num"],
    ["vs CGAL", "num"],
    ["vs Manifold", "num"],
  ] as const) {
    const th = el("th", cls || undefined, h);
    th.setAttribute("scope", "col");
    htr.append(th);
  }
  thead.append(htr);
  table.append(thead);

  const tbody = el("tbody");
  for (const r of SHOOTOUT) {
    const tr = el("tr");
    tr.append(el("th", undefined, r.model));
    (tr.lastChild as HTMLElement).setAttribute("scope", "row");
    tr.append(
      el("td", "num", fmtTime(r.openrscad)),
      el("td", "num", fmtTime(r.cgal)),
      el("td", "num", fmtTime(r.mfld)),
      el("td", "num strong", fmtX(r.cgal / r.openrscad)),
      el("td", "num strong", fmtX(r.mfld / r.openrscad)),
    );
    tbody.append(tr);
  }
  table.append(tbody);

  const cap = el(
    "p",
    "mk-table-cap",
    "Full-process render time, best of 3 runs (lower is better).",
  );
  host.replaceChildren(table, cap);
}

function renderShootout() {
  const chart = document.getElementById("shootout-chart");
  const table = document.getElementById("shootout-table");
  if (chart) renderChart(chart);
  if (table) renderTable(table);
}

wireTheme();
wireMenu();
wireHero();
renderShootout();
void wirePrimaryDownload();
