import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { exportShape2D, exportShape3D, render, renderToGlb } from "../dist/node.js";

test("the Node facade renders and exports through the complete Wasm ABI", async () => {
  const cube = await render("cube(2);");
  assert.equal(cube.ok, true);
  assert.equal(cube.triangleCount, 12);

  const svg = await exportShape2D("square(2);", "svg");
  assert.ok(svg.includes("<svg "));
});

test("the Node facade forwards binary imports and font files", async () => {
  const [stl, font] = await Promise.all([
    readFile(new URL("../../../corpus/geom/cube.stl", import.meta.url)),
    readFile(
      new URL("../../../crates/openrscad-eval/fonts/LiberationSans-Regular.ttf", import.meta.url),
    ),
  ]);

  const imported = await render('import("cube.stl");', {
    binaryFiles: { "cube.stl": stl },
  });
  assert.equal(imported.ok, true);
  assert.ok(imported.triangleCount > 0);

  const text = await render('linear_extrude(1) text("A", font="Liberation Sans");', {
    fontFiles: [font],
  });
  assert.equal(text.ok, true);
  assert.ok(text.triangleCount > 0);
});

test("the Node facade exports deterministic multipart GLB with optional native edges", async () => {
  const source = 'color("red") { cube(1); translate([0,0,2]) cube(1); }';
  const plain = await exportShape3D(source, "glb");
  assert.equal(plain.ok, true, plain.error);
  assert.equal(Buffer.from(plain.bytes.subarray(0, 4)).toString(), "glTF");
  const jsonLength = new DataView(plain.bytes.buffer, plain.bytes.byteOffset + 12, 4).getUint32(
    0,
    true,
  );
  const json = JSON.parse(new TextDecoder().decode(plain.bytes.subarray(20, 20 + jsonLength)));
  assert.deepEqual(
    json.nodes.map((node) => node.name),
    ["#FF0000FF Shape 1", "#FF0000FF Shape 2"],
  );
  assert.ok(json.meshes.every((mesh) => mesh.primitives.length === 1));

  const edged = await exportShape3D(source, "glb", {
    includeEdges: true,
  });
  assert.equal(edged.ok, true, edged.error);
  const edgedLength = new DataView(
    edged.bytes.buffer,
    edged.bytes.byteOffset + 12,
    4,
  ).getUint32(0, true);
  const edgedJson = JSON.parse(
    new TextDecoder().decode(edged.bytes.subarray(20, 20 + edgedLength)),
  );
  assert.ok(edgedJson.meshes.every((mesh) => mesh.primitives.length === 2));
});

test("the Node facade preserves authored module hierarchy independently of fallback naming", async () => {
  const source = `
    module lower_frame() { cube([2, 2, 1]); }
    module roof_frame() { translate([0, 0, 1]) cube([2, 2, 1]); }
    module assembly() { lower_frame(); roof_frame(); }
    assembly();
  `;
  const output = await exportShape3D(source, "glb");
  assert.equal(output.ok, true, output.error);
  const jsonLength = new DataView(
    output.bytes.buffer,
    output.bytes.byteOffset + 12,
    4,
  ).getUint32(0, true);
  const json = JSON.parse(
    new TextDecoder().decode(output.bytes.subarray(20, 20 + jsonLength)),
  );

  assert.deepEqual(json.scenes[0].nodes, [0]);
  assert.deepEqual(
    json.nodes.map((node) => node.name),
    ["Assembly", "Lower Frame", "Roof Frame"],
  );
  assert.deepEqual(json.nodes[0].children, [1, 2]);
  assert.equal(json.nodes[2].extras.openrscad.moduleName, "roof_frame");
});

test("renderToGlb owns preview semantics while exportShape3D owns export semantics", async () => {
  const source = "if ($preview) cube(1); else cube(2);";
  const rendered = await renderToGlb(source, { params: { $preview: false } });
  const exported = await exportShape3D(source, "glb", { params: { $preview: true } });

  assert.equal(rendered.ok, true, rendered.error);
  assert.equal(exported.ok, true, exported.error);
  assert.equal(rendered.volume, 1);
  assert.equal(exported.volume, 8);
});

test("the Node facade exports every supported 3D format", async () => {
  for (const format of ["stl", "off", "obj", "3mf", "amf", "glb"]) {
    const output = await exportShape3D("cube(1);", format);
    assert.equal(output.ok, true, `${format}: ${output.error}`);
    assert.ok(output.bytes.length > 0, format);
  }
});
