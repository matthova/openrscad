#!/usr/bin/env node
// Benchmark render() against native GLB export with edges disabled/enabled.
// Usage: node scripts/benchmark-export-shape3d.mjs [--samples 30] [--report FILE]
//        [--scad ENTRY.scad ...]

import { readFile, writeFile } from "node:fs/promises";
import { basename, dirname, extname, relative, resolve, sep } from "node:path";
import process from "node:process";

import { glob } from "glob";

import * as publicApi from "../packages/npm/dist/node.js";
import {
  builtInFixtures,
  collectArtifactParity,
  runExportShape3DBenchmark,
} from "../benchmarks/export-shape3d-benchmark.mjs";

const parseArgs = (args) => {
  const result = { samples: 30, report: undefined, scad: [] };
  for (let index = 0; index < args.length; index += 1) {
    if (args[index] === "--samples") result.samples = Number(args[++index]);
    else if (args[index] === "--report") result.report = resolve(args[++index]);
    else if (args[index] === "--scad") result.scad.push(resolve(args[++index]));
    else throw new Error(`unknown argument: ${args[index]}`);
  }
  if (!Number.isInteger(result.samples) || result.samples < 1) throw new Error("invalid --samples");
  return result;
};

const posix = (path) => path.split(sep).join("/");
const text = new Set([".scad", ".dxf", ".svg"]);
const binary = new Set([".stl", ".3mf", ".off", ".obj", ".amf"]);
const fonts = new Set([".ttf", ".otf", ".ttc"]);

const externalFixture = async (entry) => {
  const root = dirname(entry);
  const assets = await glob("**/*", { absolute: true, cwd: root, nodir: true });
  const options = { files: {}, binaryFiles: {}, fontFiles: [] };
  for (const asset of assets) {
    if (asset === entry) continue;
    const key = posix(relative(root, asset));
    const extension = extname(asset).toLowerCase();
    if (text.has(extension)) options.files[key] = await readFile(asset, "utf8");
    else if (binary.has(extension)) options.binaryFiles[key] = await readFile(asset);
    else if (fonts.has(extension)) options.fontFiles.push(await readFile(asset));
  }
  return { name: `external:${entry}`, source: await readFile(entry, "utf8"), options };
};

const main = async () => {
  const options = parseArgs(process.argv.slice(2));
  const fixtures = [...builtInFixtures];
  const [font, stl] = await Promise.all([
    readFile(new URL("../crates/openrscad-eval/fonts/LiberationSans-Regular.ttf", import.meta.url)),
    readFile(new URL("../corpus/geom/cube.stl", import.meta.url)),
  ]);
  fixtures.find(({ name }) => name === "text").options = { fontFiles: [font] };
  fixtures.push({
    name: "binary-import",
    source: 'import("cube.stl");',
    options: { binaryFiles: { "cube.stl": stl } },
  });
  for (const entry of options.scad) fixtures.push(await externalFixture(entry));
  for (const fixture of fixtures) {
    fixture.options = {
      ...fixture.options,
      params: { ...fixture.options?.params, $fn: 0, $fa: 12, $fs: 2 },
    };
  }
  const started = new Date().toISOString();
  const raw = await import("../packages/npm/pkg/node/openrscad.js");
  const api = {
    ...publicApi,
    ...(typeof raw.take_last_benchmark_profile === "function"
      ? {
          takeLastBenchmarkProfile: () =>
            JSON.parse(raw.take_last_benchmark_profile() || "null"),
        }
      : {}),
  };
  const benchmark = await runExportShape3DBenchmark({ api, fixtures, samples: options.samples });
  const parity = await collectArtifactParity({ api, fixtures });
  const report = {
    schemaVersion: 1,
    started,
    finished: new Date().toISOString(),
    host: "node",
    node: process.version,
    platform: `${process.platform}-${process.arch}`,
    samplesPerGroup: options.samples,
    fixtures: fixtures.map(({ name }) => name),
    internalTimings:
      "Present only when packages/npm was built with build:wasm:profile; production builds contain no timers or profiling ABI.",
    parity,
    ...benchmark,
  };
  const json = `${JSON.stringify(report, null, 2)}\n`;
  if (options.report) await writeFile(options.report, json);
  console.log(json);
};

await main();
