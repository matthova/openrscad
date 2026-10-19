// Point the tab favicon (and the mobile theme-color) at the active light/dark
// theme. The static <link rel="icon"> in each HTML head is favicon.svg, which
// already follows the OS on its own via a prefers-color-scheme media query.
// This runs once a theme is resolved — including a manual toggle that overrides
// the OS — so the icon tracks the *site* theme, not just the OS appearance.

const THEME_COLOR = { dark: "#14161b", light: "#fbfaf7" } as const;

export function setFavicon(theme: "dark" | "light") {
  const href = theme === "light" ? "./favicon-light.svg" : "./favicon-dark.svg";
  let link = document.querySelector<HTMLLinkElement>('link[rel="icon"]');
  if (!link) {
    link = document.createElement("link");
    link.rel = "icon";
    link.type = "image/svg+xml";
    document.head.appendChild(link);
  }
  link.href = href;

  const meta = document.querySelector<HTMLMetaElement>(
    'meta[name="theme-color"]',
  );
  if (meta) meta.content = THEME_COLOR[theme];
}
