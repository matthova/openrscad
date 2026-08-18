const encoder = new TextEncoder();

export const builtInFixtures = [
  { name: "cube", source: "cube(10);" },
  {
    name: "separated-same-color",
    source: 'color("red") { cube(10); translate([0,0,20]) cube(10); }',
  },
  {
    name: "connected-multicolor",
    source: 'color("red") cube(10); translate([5,0,0]) color("blue") cube(10);',
  },
  {
    name: "colored-difference",
    source: 'color("red") difference() { cube(10); translate([5,0,0]) color("blue") cube(10); }',
  },
  {
    name: "preview-modifiers",
    source: '# translate([0,0,12]) cube(10); % translate([0,0,-12]) cube(10); cube(10);',
  },
  {
    name: "text",
    source: 'linear_extrude(2) text("OpenRSCAD", size=8);',
  },
];

const paths = [
  { id: "E1", run: (api, fixture) => api.render(fixture.source, fixture.options) },
  {
    id: "E2-",
    run: (api, fixture) =>
      api.renderToGlb(fixture.source, {
        ...fixture.options,
        includeEdges: false,
      }),
  },
  {
    id: "E2+",
    run: (api, fixture) =>
      api.renderToGlb(fixture.source, {
        ...fixture.options,
        includeEdges: true,
      }),
  },
  {
    id: "E3-",
    run: (api, fixture) =>
      api.exportShape3D(fixture.source, "glb", {
        ...fixture.options,
        includeEdges: false,
      }),
  },
  {
    id: "E3+",
    run: (api, fixture) =>
      api.exportShape3D(fixture.source, "glb", {
        ...fixture.options,
        includeEdges: true,
      }),
  },
  {
    id: "E4",
    run: (api, fixture) => api.exportShape3D(fixture.source, "3mf", fixture.options),
  },
];

const glbJson = (bytes) => {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const length = view.getUint32(12, true);
  return JSON.parse(new TextDecoder().decode(bytes.subarray(20, 20 + length)));
};

const measureOutput = (path, output) => {
  if (!output.ok) throw new Error(`${path}: ${output.error}`);
  if (path === "E1") {
    const bytes =
      output.positions.byteLength +
      output.normals.byteLength +
      output.preview.positions.byteLength +
      output.preview.normals.byteLength +
      output.provenance.positions.byteLength +
      output.provenance.normals.byteLength +
      encoder.encode(output.preview.groups + output.provenance.groups).byteLength;
    return {
      bytes,
      lines: 0,
      nodes: 1,
      triangles: output.triangleCount,
      vertices: output.vertexCount,
      volume: output.volume,
    };
  }
  if (path === "E4") {
    return {
      bytes: output.bytes.byteLength,
      lines: 0,
      nodes: 0,
      triangles: output.triangleCount,
      vertices: output.vertexCount,
      volume: output.volume,
    };
  }
  const json = glbJson(output.bytes);
  const primitives = (json.meshes ?? []).flatMap((mesh) => mesh.primitives ?? []);
  const lines = primitives
    .filter((primitive) => primitive.mode === 1)
    .reduce((total, primitive) => total + (json.accessors?.[primitive.indices]?.count ?? 0) / 2, 0);
  if (path.endsWith("-") && lines !== 0) throw new Error(`${path}: unexpected lines`);
  if (path.endsWith("+") && output.triangleCount > 0 && lines === 0) {
    throw new Error(`${path}: missing feature lines`);
  }
  return {
    bytes: output.bytes.byteLength,
    lines,
    nodes: json.nodes?.length ?? 0,
    triangles: output.triangleCount,
    vertices: output.vertexCount,
    volume: output.volume,
  };
};

const heapBytes = () => {
  if (typeof process !== "undefined" && process.memoryUsage) return process.memoryUsage().heapUsed;
  return globalThis.performance?.memory?.usedJSHeapSize ?? null;
};

const median = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
};

const quantile = (values, q) => {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * q) - 1)];
};

const seedFor = (value) => {
  let seed = 2166136261;
  for (const byte of encoder.encode(value)) seed = Math.imul(seed ^ byte, 16777619);
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

const summarize = (samples) => {
  const groups = new Map();
  for (const sample of samples) {
    const key = `${sample.fixture}\0${sample.path}\0${sample.cache}`;
    const group = groups.get(key) ?? [];
    group.push(sample);
    groups.set(key, group);
  }
  return [...groups.entries()].map(([key, group]) => {
    const values = group.map((sample) => sample.durationMs);
    const mean = values.reduce((sum, value) => sum + value, 0) / values.length;
    const variance =
      values.reduce((sum, value) => sum + (value - mean) ** 2, 0) / values.length;
    const profileKeys = [
      ...new Set(group.flatMap((sample) => Object.keys(sample.profile ?? {}))),
    ].sort();
    const profileMedians = Object.fromEntries(
      profileKeys.flatMap((profileKey) => {
        const profileValues = group
          .map((sample) => sample.profile?.[profileKey])
          .filter((value) => typeof value === "number");
        return profileValues.length ? [[profileKey, median(profileValues)]] : [];
      }),
    );
    return {
      fixture: group[0].fixture,
      path: group[0].path,
      cache: group[0].cache,
      samples: group.length,
      medianMs: median(values),
      p95Ms: quantile(values, 0.95),
      median95CiMs: bootstrapMedian(values, key),
      coefficientOfVariation: mean === 0 ? 0 : Math.sqrt(variance) / mean,
      medianBytes: median(group.map((sample) => sample.output.bytes)),
      medianLines: median(group.map((sample) => sample.output.lines)),
      medianNodes: median(group.map((sample) => sample.output.nodes)),
      medianHeapDeltaBytes: median(
        group.flatMap((sample) =>
          typeof sample.heapDeltaBytes === "number" ? [sample.heapDeltaBytes] : [],
        ),
      ),
      peakWasmLinearMemoryAfterBytes: Math.max(
        0,
        ...group.map((sample) => sample.profile?.wasmMemoryAfterBytes ?? 0),
      ),
      structuredCacheHitRate:
        group.filter((sample) => sample.profile?.structuredCacheHit === true).length /
        group.length,
      profileMedians,
    };
  });
};

const compare = (summary) => {
  const groups = new Map();
  for (const entry of summary) {
    const key = `${entry.fixture}\0${entry.cache}`;
    const group = groups.get(key) ?? {};
    group[entry.path] = entry;
    groups.set(key, group);
  }
  return [...groups.entries()].map(([key, group]) => {
    const [fixture, cache] = key.split("\0");
    const e1 = group.E1?.medianMs;
    const ratio = (path) => (e1 ? group[path]?.medianMs / e1 : null);
    return {
      fixture,
      cache,
      ratiosToRender: {
        "E2-": ratio("E2-"),
        "E2+": ratio("E2+"),
        "E3-": ratio("E3-"),
        "E3+": ratio("E3+"),
        E4: ratio("E4"),
      },
      edgeDeltaMs: {
        preview:
          group["E2+"] && group["E2-"]
            ? group["E2+"].medianMs - group["E2-"].medianMs
            : null,
        export:
          group["E3+"] && group["E3-"]
            ? group["E3+"].medianMs - group["E3-"].medianMs
            : null,
      },
    };
  });
};

const sha256 = async (bytes) => {
  const webCrypto = globalThis.crypto ?? (await import("node:crypto")).webcrypto;
  const digest = await webCrypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)].map((value) => value.toString(16).padStart(2, "0")).join("");
};

/** Capture deterministic artifact hashes for browser/Node parity outside timed samples. */
export const collectArtifactParity = async ({ api, fixtures }) => {
  const parity = [];
  for (const fixture of fixtures) {
    for (const includeEdges of [false, true]) {
      await api.clearCache();
      const output = await api.renderToGlb(fixture.source, {
        ...fixture.options,
        includeEdges,
      });
      if (!output.ok) throw new Error(`${fixture.name} render GLB parity: ${output.error}`);
      parity.push({
        fixture: fixture.name,
        format: "render-glb",
        includeEdges,
        bytes: output.bytes.byteLength,
        sha256: await sha256(output.bytes),
      });
    }
    for (const includeEdges of [false, true]) {
      await api.clearCache();
      const output = await api.exportShape3D(fixture.source, "glb", {
        ...fixture.options,
        includeEdges,
      });
      if (!output.ok) throw new Error(`${fixture.name} export GLB parity: ${output.error}`);
      parity.push({
        fixture: fixture.name,
        format: "export-glb",
        includeEdges,
        bytes: output.bytes.byteLength,
        sha256: await sha256(output.bytes),
      });
    }
    await api.clearCache();
    const output = await api.exportShape3D(fixture.source, "3mf", fixture.options);
    if (!output.ok) throw new Error(`${fixture.name} 3MF parity export: ${output.error}`);
    parity.push({
      fixture: fixture.name,
      format: "3mf",
      includeEdges: false,
      bytes: output.bytes.byteLength,
      sha256: await sha256(output.bytes),
    });
  }
  return parity;
};

export const runExportShape3DBenchmark = async ({ api, fixtures = builtInFixtures, samples = 30 }) => {
  const raw = [];
  for (const fixture of fixtures) {
    for (const path of paths) {
      await api.clearCache();
      measureOutput(path.id, await path.run(api, fixture));
    }
    for (let iteration = 0; iteration < samples; iteration += 1) {
      const ordered = paths.map((_, index) => paths[(index + iteration) % paths.length]);
      for (const cache of ["cold", "warm"]) {
        for (const path of ordered) {
          await api.clearCache();
          if (cache === "warm") await path.run(api, fixture);
          const beforeHeapBytes = heapBytes();
          const start = performance.now();
          const result = await path.run(api, fixture);
          const durationMs = performance.now() - start;
          const profile = api.takeLastBenchmarkProfile?.() ?? null;
          if (profile) {
            profile.transferAndFacadeMs = Math.max(0, durationMs - (profile.rustTotalMs ?? 0));
            profile.wasmMemoryDeltaBytes =
              profile.wasmMemoryAfterBytes - profile.wasmMemoryBeforeBytes;
          }
          raw.push({
            fixture: fixture.name,
            path: path.id,
            cache,
            iteration,
            durationMs,
            heapDeltaBytes:
              beforeHeapBytes === null || heapBytes() === null ? null : heapBytes() - beforeHeapBytes,
            profile,
            output: measureOutput(path.id, result),
          });
        }
      }
    }
  }
  const summary = summarize(raw);
  return { raw, summary, comparisons: compare(summary) };
};
