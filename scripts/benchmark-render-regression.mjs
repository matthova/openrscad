#!/usr/bin/env node

// Compare the raw render() path between two matched Node Wasm builds.

import { readFile, writeFile } from "node:fs/promises";
import { cpus, platform, release } from "node:os";
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import process from "node:process";
import { builtInFixtures } from "../benchmarks/export-shape3d-benchmark.mjs";

const parseArgs = (values) => {
  const result = { baseline: "", current: "", entry: "", report: "", samples: 30 };
  for (let index = 0; index < values.length; index += 2) {
    const value = values[index + 1];
    if (!value) throw new Error(`Missing value for ${values[index]}`);
    if (values[index] === "--baseline") result.baseline = resolve(value);
    else if (values[index] === "--current") result.current = resolve(value);
    else if (values[index] === "--entry") result.entry = resolve(value);
    else if (values[index] === "--report") result.report = resolve(value);
    else if (values[index] === "--samples") result.samples = Number(value);
    else throw new Error(`Unknown argument: ${values[index]}`);
  }
  if (!result.baseline || !result.current || !result.entry || !result.report) {
    throw new Error("Required: --baseline --current --entry --report [--samples 30]");
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

const quantile = (input, value) =>
  [...input].sort((left, right) => left - right)[Math.min(input.length - 1, Math.ceil(input.length * value) - 1)];

const seedFor = (value) => {
  let seed = 2166136261;
  for (const byte of new TextEncoder().encode(value)) seed = Math.imul(seed ^ byte, 16777619);
  return seed >>> 0;
};

const bootstrapMedian = (values, key) => {
  let seed = seedFor(key);
  const random = () => {
    seed ^= seed << 13;
    seed ^= seed >>> 17;
    seed ^= seed << 5;
    return (seed >>> 0) / 0x1_0000_0000;
  };
  const medians = [];
  for (let sample = 0; sample < 1_000; sample += 1) {
    medians.push(median(values.map(() => values[Math.floor(random() * values.length)])));
  }
  return [quantile(medians, 0.025), quantile(medians, 0.975)];
};

const toBase64 = (bytes) => Buffer.from(bytes).toString("base64");

const requestArrays = (options = {}) => {
  const params = Object.entries(options.params ?? {});
  const files = Object.entries(options.files ?? {});
  const binaryFiles = Object.entries(options.binaryFiles ?? {});
  return [
    params.map(([name]) => name),
    params.map(([, value]) => String(value)),
    files.map(([name]) => name),
    files.map(([, value]) => value),
    binaryFiles.map(([name]) => name),
    binaryFiles.map(([, value]) => toBase64(value)),
    (options.fontFiles ?? []).map(toBase64),
  ];
};

const copyBytes = (value) => new Uint8Array(value.buffer, value.byteOffset, value.byteLength).slice();

const render = (engine, fixture) => {
  const result = engine.render_with_files(fixture.source, ...requestArrays(fixture.options));
  try {
    return {
      ok: result.ok,
      error: result.error,
      positions: copyBytes(result.positions),
      normals: copyBytes(result.normals),
      previewPositions: copyBytes(result.preview_positions),
      previewNormals: copyBytes(result.preview_normals),
      provenancePositions: copyBytes(result.provenance_positions),
      provenanceNormals: copyBytes(result.provenance_normals),
      groups: result.groups,
      provenance: result.provenance,
      diagnostics: result.diagnostics,
      is2d: result.is_2d,
      triangleCount: result.triangle_count,
      vertexCount: result.vertex_count,
      volume: result.volume,
      area: result.area,
    };
  } finally {
    result.free();
  }
};

const assertEqualBytes = (options) => {
  if (options.left.byteLength !== options.right.byteLength) {
    throw new Error(`${options.fixture}: ${options.channel} byte length changed`);
  }
  for (let index = 0; index < options.left.byteLength; index += 1) {
    if (options.left[index] !== options.right[index]) {
      throw new Error(`${options.fixture}: ${options.channel} changed at byte ${index}`);
    }
  }
};

const assertSemanticParity = (fixture, baseline, current) => {
  for (const field of [
    "ok",
    "error",
    "groups",
    "provenance",
    "diagnostics",
    "is2d",
    "triangleCount",
    "vertexCount",
    "volume",
    "area",
  ]) {
    if (!Object.is(baseline[field], current[field])) {
      throw new Error(`${fixture}: ${field} changed (${baseline[field]} !== ${current[field]})`);
    }
  }
  for (const channel of [
    "positions",
    "normals",
    "previewPositions",
    "previewNormals",
    "provenancePositions",
    "provenanceNormals",
  ]) {
    assertEqualBytes({ fixture, channel, left: baseline[channel], right: current[channel] });
  }
};

const args = parseArgs(process.argv.slice(2));
const [baseline, current] = await Promise.all([
  import(pathToFileURL(args.baseline)),
  import(pathToFileURL(args.current)),
]);
const [font, stl] = await Promise.all([
  readFile(resolve("crates/openrscad-eval/fonts/LiberationSans-Regular.ttf")),
  readFile(resolve("corpus/geom/cube.stl")),
]);
const entryRoot = dirname(args.entry);
const fileNames = [
  "lib/bearing.scad",
  "lib/cap.scad",
  "lib/carrier.scad",
  "lib/gear.scad",
  "lib/housing.scad",
  "lib/params.scad",
  "lib/planet.scad",
  "lib/sun.scad",
];
const fixtures = structuredClone(builtInFixtures);
fixtures.find(({ name }) => name === "text").options = { fontFiles: [new Uint8Array(font)] };
fixtures.push({
  name: "binary-import",
  source: 'import("cube.stl");',
  options: { binaryFiles: { "cube.stl": new Uint8Array(stl) } },
});
fixtures.push({
  name: "planetary-gearbox",
  source: await readFile(args.entry, "utf8"),
  options: {
    files: Object.fromEntries(
      await Promise.all(fileNames.map(async (name) => [name, await readFile(resolve(entryRoot, name), "utf8")])),
    ),
  },
});
for (const fixture of fixtures) {
  fixture.options = {
    ...fixture.options,
    params: { ...fixture.options?.params, $fn: 0, $fa: 12, $fs: 2 },
  };
}

const contenders = { baseline, current };
const parity = [];
for (const fixture of fixtures) {
  baseline.clear_cache();
  current.clear_cache();
  const baselineOutput = render(baseline, fixture);
  const currentOutput = render(current, fixture);
  assertSemanticParity(fixture.name, baselineOutput, currentOutput);
  parity.push({
    fixture: fixture.name,
    triangles: currentOutput.triangleCount,
    vertices: currentOutput.vertexCount,
    volume: currentOutput.volume,
    bytes: currentOutput.positions.byteLength + currentOutput.normals.byteLength,
  });
}

const raw = [];
for (const fixture of fixtures) {
  for (let iteration = 0; iteration < args.samples; iteration += 1) {
    const order = iteration % 2 === 0 ? ["baseline", "current"] : ["current", "baseline"];
    for (const candidate of order) {
      const engine = contenders[candidate];
      for (const cache of ["cold", "warm"]) {
        engine.clear_cache();
        if (cache === "warm") render(engine, fixture);
        const start = performance.now();
        const output = render(engine, fixture);
        raw.push({
          fixture: fixture.name,
          candidate,
          cache,
          iteration,
          durationMs: performance.now() - start,
          triangles: output.triangleCount,
          vertices: output.vertexCount,
        });
      }
    }
  }
  console.log(`[${fixtures.indexOf(fixture) + 1}/${fixtures.length}] ${fixture.name}`);
}

const summary = [];
for (const fixture of fixtures) {
  for (const candidate of ["baseline", "current"]) {
    for (const cache of ["cold", "warm"]) {
      const values = raw
        .filter((sample) => sample.fixture === fixture.name && sample.candidate === candidate && sample.cache === cache)
        .map(({ durationMs }) => durationMs);
      summary.push({
        fixture: fixture.name,
        candidate,
        cache,
        samples: values.length,
        medianMs: median(values),
        p95Ms: p95(values),
        median95CiMs: bootstrapMedian(values, `${fixture.name}\0${candidate}\0${cache}`),
      });
    }
  }
}

const comparisons = fixtures.flatMap((fixture) =>
  ["cold", "warm"].map((cache) => {
    const baselineEntry = summary.find(
      (entry) => entry.fixture === fixture.name && entry.candidate === "baseline" && entry.cache === cache,
    );
    const currentEntry = summary.find(
      (entry) => entry.fixture === fixture.name && entry.candidate === "current" && entry.cache === cache,
    );
    return {
      fixture: fixture.name,
      cache,
      medianDeltaPercent: ((currentEntry.medianMs / baselineEntry.medianMs) - 1) * 100,
      p95DeltaPercent: ((currentEntry.p95Ms / baselineEntry.p95Ms) - 1) * 100,
    };
  }),
);

await writeFile(
  args.report,
  `${JSON.stringify(
    {
      schemaVersion: 1,
      createdAt: new Date().toISOString(),
      environment: { platform: platform(), release: release(), cpu: cpus()[0]?.model, node: process.version },
      methodology: {
        samples: args.samples,
        order: "candidate first position alternates by iteration; contenders execute serially",
        cache: "cold clears immediately before measurement; warm clears then primes only that candidate",
        boundary: "raw Node wasm-bindgen render_with_files to avoid the parent npm facade ABI defect",
      },
      artifacts: { baseline: args.baseline, current: args.current },
      parity,
      raw,
      summary,
      comparisons,
    },
    null,
    2,
  )}\n`,
);
console.log(`Wrote ${args.report}`);
