#!/usr/bin/env node

// Compare Wasm payloads and fresh-process Node initialization.

import { readFile, writeFile } from "node:fs/promises";
import { cpus, platform, release } from "node:os";
import { resolve } from "node:path";
import process from "node:process";
import { pathToFileURL } from "node:url";
import { brotliCompressSync, constants, gzipSync } from "node:zlib";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";

const parseArgs = (values) => {
  const result = { baseline: "", current: "", report: "", samples: 100 };
  for (let index = 0; index < values.length; index += 2) {
    const value = values[index + 1];
    if (!value) throw new Error(`Missing value for ${values[index]}`);
    if (values[index] === "--baseline") result.baseline = resolve(value);
    else if (values[index] === "--current") result.current = resolve(value);
    else if (values[index] === "--report") result.report = resolve(value);
    else if (values[index] === "--samples") result.samples = Number(value);
    else throw new Error(`Unknown argument: ${values[index]}`);
  }
  if (!result.baseline || !result.current || !result.report) {
    throw new Error("Required: --baseline --current --report [--samples 100]");
  }
  if (!Number.isInteger(result.samples) || result.samples < 1) {
    throw new Error("--samples must be a positive integer");
  }
  return result;
};

const median = (input) => {
  const values = [...input].sort((left, right) => left - right);
  const middle = Math.floor(values.length / 2);
  return values.length % 2 ? values[middle] : (values[middle - 1] + values[middle]) / 2;
};

const p95 = (input) => [...input].sort((left, right) => left - right)[Math.ceil(input.length * 0.95) - 1];

const percent = (current, baseline) => ((current / baseline) - 1) * 100;

const args = parseArgs(process.argv.slice(2));
const artifacts = {
  baseline: args.baseline,
  current: args.current,
};
const sizes = {};
for (const [name, root] of Object.entries(artifacts)) {
  const [wasm, glue] = await Promise.all([
    readFile(resolve(root, "openrscad_bg.wasm")),
    readFile(resolve(root, "openrscad.js")),
  ]);
  sizes[name] = {
    wasmRawBytes: wasm.byteLength,
    wasmGzipBytes: gzipSync(wasm, { level: 9 }).byteLength,
    wasmBrotliBytes: brotliCompressSync(wasm, {
      params: { [constants.BROTLI_PARAM_QUALITY]: 11 },
    }).byteLength,
    glueBytes: glue.byteLength,
    wasmSha256: createHash("sha256").update(wasm).digest("hex"),
  };
}

const raw = [];
const names = Object.keys(artifacts);
for (let iteration = 0; iteration < args.samples; iteration += 1) {
  const offset = iteration % names.length;
  const order = [...names.slice(offset), ...names.slice(0, offset)];
  for (const name of order) {
    const moduleUrl = pathToFileURL(resolve(artifacts[name], "openrscad.js")).href;
    const code = `const start = performance.now(); await import(${JSON.stringify(moduleUrl)}); console.log(JSON.stringify({ importMs: performance.now() - start, rssBytes: process.memoryUsage().rss }));`;
    const processStart = performance.now();
    const child = spawnSync(process.execPath, ["--input-type=module", "--eval", code], {
      encoding: "utf8",
    });
    const processMs = performance.now() - processStart;
    if (child.status !== 0) {
      throw new Error(`${name} startup failed: ${child.stderr || child.stdout}`);
    }
    raw.push({ name, iteration, processMs, ...JSON.parse(child.stdout.trim()) });
  }
}

const summary = names.map((name) => {
  const samples = raw.filter((sample) => sample.name === name);
  return {
    name,
    samples: samples.length,
    importMedianMs: median(samples.map(({ importMs }) => importMs)),
    importP95Ms: p95(samples.map(({ importMs }) => importMs)),
    processMedianMs: median(samples.map(({ processMs }) => processMs)),
    processP95Ms: p95(samples.map(({ processMs }) => processMs)),
    rssMedianBytes: median(samples.map(({ rssBytes }) => rssBytes)),
  };
});
const baselineSummary = summary.find(({ name }) => name === "baseline");
const currentSummary = summary.find(({ name }) => name === "current");
const comparison = {
  wasmRawPercent: percent(sizes.current.wasmRawBytes, sizes.baseline.wasmRawBytes),
  wasmGzipPercent: percent(sizes.current.wasmGzipBytes, sizes.baseline.wasmGzipBytes),
  wasmBrotliPercent: percent(sizes.current.wasmBrotliBytes, sizes.baseline.wasmBrotliBytes),
  importMedianPercent: percent(currentSummary.importMedianMs, baselineSummary.importMedianMs),
  processMedianPercent: percent(currentSummary.processMedianMs, baselineSummary.processMedianMs),
};

await writeFile(
  args.report,
  `${JSON.stringify(
    {
      schemaVersion: 1,
      createdAt: new Date().toISOString(),
      environment: { platform: platform(), release: release(), cpu: cpus()[0]?.model, node: process.version },
      methodology: {
        samples: args.samples,
        order: "artifact first position rotates by iteration; every measurement uses a fresh Node process",
        import: "dynamic import duration measured inside the child process",
        process: "spawn-to-exit wall time measured by the parent process",
        compression: "gzip level 9 and Brotli quality 11",
      },
      artifacts,
      sizes,
      summary,
      comparison,
      raw,
    },
    null,
    2,
  )}\n`,
);
console.log(`Wrote ${args.report}`);
