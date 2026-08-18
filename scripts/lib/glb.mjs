// Structural counts read straight out of a GLB, shared by the corpus
// validators and their tests.

// A patch boundary on a closed surface never just stops: it either closes on
// itself or runs into a junction where more patches meet. So a vertex touched by
// exactly one segment is a seam that eroded halfway along, which a segment total
// cannot see — the Steinmetz solid fell from 48 segments to 24 without its count
// ever leaving a plausible range. Higher valences are ordinary: a cube corner is
// three. Counted per primitive, so node transforms never enter, and keyed by
// position because a seam closes geometrically, not by index.
export const danglingEndpoints = (positions, indices) => {
  const valence = new Map();
  for (const index of indices) {
    const key = `${positions[index * 3]},${positions[index * 3 + 1]},${positions[index * 3 + 2]}`;
    valence.set(key, (valence.get(key) ?? 0) + 1);
  }
  let dangling = 0;
  for (const count of valence.values()) if (count === 1) dangling += 1;
  return dangling;
};

export const glbCounts = (bytes) => {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (view.getUint32(0, true) !== 0x46546c67) throw new Error("artifact is not a GLB");
  const jsonLength = view.getUint32(12, true);
  const json = JSON.parse(new TextDecoder().decode(bytes.subarray(20, 20 + jsonLength)));
  const binaryOffset = 20 + jsonLength + ((4 - (jsonLength % 4)) % 4) + 8;
  const read = (accessorIndex, ArrayType) => {
    const accessor = json.accessors?.[accessorIndex];
    const bufferView = json.bufferViews?.[accessor?.bufferView];
    if (!accessor || !bufferView) return new ArrayType(0);
    const components = accessor.type === "VEC3" ? 3 : 1;
    const start = binaryOffset + (bufferView.byteOffset ?? 0) + (accessor.byteOffset ?? 0);
    return new ArrayType(bytes.buffer.slice(
      bytes.byteOffset + start,
      bytes.byteOffset + start + accessor.count * components * ArrayType.BYTES_PER_ELEMENT,
    ));
  };
  let triangleCount = 0;
  let lineCount = 0;
  let danglingEndpointCount = 0;
  for (const mesh of json.meshes ?? []) {
    for (const primitive of mesh.primitives ?? []) {
      const count = json.accessors?.[primitive.indices]?.count ?? 0;
      if ((primitive.mode ?? 4) === 4) triangleCount += count / 3;
      if (primitive.mode === 1) {
        lineCount += count / 2;
        danglingEndpointCount += danglingEndpoints(
          read(primitive.attributes?.POSITION, Float32Array),
          read(primitive.indices, Uint32Array),
        );
      }
    }
  }
  return {
    lineCount,
    meshCount: json.meshes?.length ?? 0,
    nodeCount: json.nodes?.length ?? 0,
    danglingEndpointCount,
    triangleCount,
  };
};
