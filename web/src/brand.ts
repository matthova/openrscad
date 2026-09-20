// Standalone entry for the brand page (brand.html, served at /openrscad/brand).
// No React — the page is static HTML; this only wires the shared chrome: the
// light/dark toggle, the mobile nav menu, and the right-click-logo shortcut
// (which, from here, just reloads this page).
import "./about.css";
import { wireTheme, wireMenu, wireLogoContextMenu } from "./mkChrome";

wireTheme();
wireMenu();
wireLogoContextMenu();
