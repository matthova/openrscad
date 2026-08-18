import assert from "node:assert/strict";
import test from "node:test";

import { makeApi } from "../dist/core.js";

const rawResult = () => ({
  ok: true,
  error: "",
  positions: new Float32Array(),
  normals: new Float32Array(),
  is_2d: false,
  triangle_count: 0,
  vertex_count: 0,
  volume: 0,
  area: 0,
  echo: "",
  warnings: "",
  geom_errors: "degraded boolean",
  diagnostics: "[]",
  preview_positions: new Float32Array(),
  preview_normals: new Float32Array(),
  groups: "",
  provenance_positions: new Float32Array(),
  provenance_normals: new Float32Array(),
  provenance: "",
  viewport: "",
  free() {},
});

const rawExportResult = () => ({
  ok: true,
  error: "",
  format: "glb",
  is_2d: false,
  triangle_count: 12,
  vertex_count: 8,
  volume: 1,
  area: 6,
  echo: "",
  warnings: "",
  geom_errors: "",
  diagnostics: "[]",
  viewport: "",
  take_bytes: () => new Uint8Array([0x67, 0x6c, 0x54, 0x46]),
  free() {},
});

test("render forwards the complete raw Wasm ABI and surfaces geometry errors", async () => {
  let args;
  const api = makeApi(
    {
      render_with_files(...received) {
        args = received;
        return rawResult();
      },
      export_2d() {
        return "";
      },
      export_3d() {
        return rawExportResult();
      },
      parameters() {
        return "{}";
      },
      version() {
        return "test";
      },
      clear_cache() {},
    },
    async () => {},
  );

  const output = await api.render("import(\"part.stl\");", {
    files: { "lib.scad": "cube(1);" },
    binaryFiles: { "part.stl": new Uint8Array([0, 1, 254, 255]) },
    fontFiles: [new Uint8Array([65, 66, 67])],
  });

  assert.equal(args.length, 8);
  assert.deepEqual(args.slice(3), [
    ["lib.scad"],
    ["cube(1);"],
    ["part.stl"],
    ["AAH+/w=="],
    ["QUJD"],
  ]);
  assert.equal(output.geomErrors, "degraded boolean");
});

test("exportShape2D forwards binary files and fonts through the raw Wasm ABI", async () => {
  let args;
  const api = makeApi(
    {
      render_with_files() {
        return rawResult();
      },
      export_2d(...received) {
        args = received;
        return "svg";
      },
      export_3d() {
        return rawExportResult();
      },
      parameters() {
        return "{}";
      },
      version() {
        return "test";
      },
      clear_cache() {},
    },
    async () => {},
  );

  assert.equal(
    await api.exportShape2D("square(1);", "svg", {
      binaryFiles: { "part.stl": new Uint8Array([1, 2, 3]) },
      fontFiles: [new Uint8Array([4, 5, 6])],
    }),
    "svg",
  );
  assert.equal(args.length, 9);
  assert.deepEqual(args.slice(5), [["part.stl"], ["AQID"], ["BAUG"], "svg"]);
});

test("exportShape3D forwards options, takes bytes once, and frees the raw result", async () => {
  let args;
  let takes = 0;
  let frees = 0;
  const api = makeApi(
    {
      render_with_files() {
        return rawResult();
      },
      export_2d() {
        return "";
      },
      export_3d(...received) {
        args = received;
        return {
          ...rawExportResult(),
          take_bytes() {
            takes += 1;
            return new Uint8Array([1, 2, 3]);
          },
          free() {
            frees += 1;
          },
        };
      },
      parameters() {
        return "{}";
      },
      version() {
        return "test";
      },
      clear_cache() {},
    },
    async () => {},
  );

  const output = await api.exportShape3D("cube(1);", "glb", {
    includeEdges: true,
    sourceUnitToMeters: 0.01,
    coordinateSystem: "z-up",
  });
  assert.deepEqual([...output.bytes], [1, 2, 3]);
  assert.deepEqual(args.slice(8), ["glb", true, 0.01, "z-up"]);
  assert.equal(takes, 1);
  assert.equal(frees, 1);
});
