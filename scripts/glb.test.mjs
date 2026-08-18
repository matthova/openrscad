import assert from "node:assert/strict";
import test from "node:test";

import { danglingEndpoints, glbCounts } from "./lib/glb.mjs";

// A minimal GLB: one mesh, one line primitive, positions and indices in the BIN
// chunk exactly as the direct writer lays them out (non-interleaved, float32
// POSITION, uint32 indices).
const buildGlb = (positions, indices) => {
  const positionBytes = new Float32Array(positions);
  const indexBytes = new Uint32Array(indices);
  const json = {
    accessors: [
      { bufferView: 0, componentType: 5126, count: positions.length / 3, type: "VEC3" },
      { bufferView: 1, componentType: 5125, count: indices.length, type: "SCALAR" },
    ],
    bufferViews: [
      { buffer: 0, byteLength: positionBytes.byteLength, byteOffset: 0, target: 34962 },
      { buffer: 0, byteLength: indexBytes.byteLength, byteOffset: positionBytes.byteLength, target: 34963 },
    ],
    buffers: [{ byteLength: positionBytes.byteLength + indexBytes.byteLength }],
    meshes: [{ primitives: [{ attributes: { POSITION: 0 }, indices: 1, mode: 1 }] }],
    nodes: [{ mesh: 0 }],
    scene: 0,
    scenes: [{ nodes: [0] }],
  };
  const jsonBytes = new TextEncoder().encode(JSON.stringify(json));
  const jsonPadding = (4 - (jsonBytes.length % 4)) % 4;
  const binary = Buffer.concat([Buffer.from(positionBytes.buffer), Buffer.from(indexBytes.buffer)]);
  const total = 12 + 8 + jsonBytes.length + jsonPadding + 8 + binary.length;
  const glb = Buffer.alloc(total);
  glb.writeUInt32LE(0x46546c67, 0);
  glb.writeUInt32LE(2, 4);
  glb.writeUInt32LE(total, 8);
  glb.writeUInt32LE(jsonBytes.length + jsonPadding, 12);
  glb.writeUInt32LE(0x4e4f534a, 16);
  Buffer.from(jsonBytes).copy(glb, 20);
  glb.fill(0x20, 20 + jsonBytes.length, 20 + jsonBytes.length + jsonPadding);
  const binaryHeader = 20 + jsonBytes.length + jsonPadding;
  glb.writeUInt32LE(binary.length, binaryHeader);
  glb.writeUInt32LE(0x004e4942, binaryHeader + 4);
  binary.copy(glb, binaryHeader + 8);
  return new Uint8Array(glb);
};

// Four corners of a unit square in the z = 0 plane.
const square = [0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0];

test("a closed loop has no dangling endpoint and is counted segment by segment", () => {
  const counts = glbCounts(buildGlb(square, [0, 1, 1, 2, 2, 3, 3, 0]));
  assert.equal(counts.lineCount, 4);
  assert.equal(counts.danglingEndpointCount, 0);
  assert.equal(counts.triangleCount, 0);
});

test("a seam eroded in the middle is caught even though its total stays plausible", () => {
  // Two of the loop's four segments gone: the count alone still looks like a
  // small model, but each surviving arc now ends in mid-air.
  const counts = glbCounts(buildGlb(square, [0, 1, 2, 3]));
  assert.equal(counts.lineCount, 2);
  assert.equal(counts.danglingEndpointCount, 4);
});

test("closure is judged by position, so a duplicated vertex still closes", () => {
  const duplicated = [...square, 0, 0, 0];
  assert.equal(danglingEndpoints(new Float32Array(duplicated), new Uint32Array([0, 1, 1, 2, 2, 3, 3, 4])), 0);
});

test("a junction where three seams meet is not a dangling end", () => {
  const star = [0, 0, 0, 1, 0, 0, -1, 0, 0, 0, 1, 0];
  // Three segments radiating from the origin: valence three at the hub, one at
  // each tip. Only the tips are dangling.
  assert.equal(danglingEndpoints(new Float32Array(star), new Uint32Array([0, 1, 0, 2, 0, 3])), 3);
});
