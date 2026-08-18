import { readFile } from "node:fs/promises";
import { dirname, extname, relative, sep } from "node:path";

import { glob } from "glob";

export const posix = (path) => path.split(sep).join("/");

const commonDirectory = (paths) => {
  let common = dirname(paths[0]);
  while (paths.some((path) => relative(common, path).startsWith(`..${sep}`))) {
    const parent = dirname(common);
    if (parent === common) break;
    common = parent;
  }
  return common;
};

export const discoverCorpus = async ({ patterns, root }) => {
  const entries = [
    ...new Set(
      (
        await Promise.all(
          patterns.map((pattern) =>
            glob(pattern, { absolute: true, cwd: root, nodir: true }),
          ),
        )
      ).flat(),
    ),
  ].sort();
  if (entries.length === 0) return null;

  const assetRoot = commonDirectory(entries);
  const assets = (
    await glob("**/*", {
      absolute: true,
      cwd: assetRoot,
      dot: true,
      nodir: true,
      ignore: ["**/.tau/**"],
    })
  ).sort();
  return { assetRoot, assets, entries };
};

// `.dat` is `surface()`'s height field — text, like the rest, and without it
// every model that reads one silently renders to nothing.
const textExtensions = new Set([".scad", ".dxf", ".svg", ".dat"]);
const binaryExtensions = new Set([".stl", ".3mf", ".off", ".obj", ".amf"]);
const fontExtensions = new Set([".ttf", ".otf", ".ttc"]);

export const requestAssets = async (entry, assets) => {
  const files = {};
  const binaryFiles = {};
  const fontFiles = [];
  for (const asset of assets) {
    if (asset === entry) continue;
    const extension = extname(asset).toLowerCase();
    const key = posix(relative(dirname(entry), asset));
    if (textExtensions.has(extension)) {
      files[key] = await readFile(asset, "utf8");
    } else if (binaryExtensions.has(extension)) {
      binaryFiles[key] = await readFile(asset);
    } else if (fontExtensions.has(extension)) {
      fontFiles.push(await readFile(asset));
    }
  }
  return { files, binaryFiles, fontFiles };
};
