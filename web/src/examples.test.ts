import { describe, it, expect } from "vitest";
import {
  EXAMPLES,
  EXAMPLE_PREFIX,
  decodeExampleRoute,
  exampleHash,
  findExampleBySlug,
} from "./examples";

describe("example routing", () => {
  it("gives every example a unique, URL-safe slug", () => {
    const slugs = EXAMPLES.map((e) => e.slug);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const slug of slugs) expect(slug).toMatch(/^[a-z0-9-]+$/);
  });

  it("round-trips an example through its `#example/<slug>` hash", () => {
    for (const ex of EXAMPLES) {
      const p = decodeExampleRoute(exampleHash(ex));
      expect(p).not.toBeNull();
      expect(p!.files).toEqual(ex.files);
      expect(p!.overrides).toEqual({});
      expect(p!.active).toBe(0);
    }
  });

  it("returns a defensive copy, not the shared example files", () => {
    const ex = EXAMPLES[0];
    const p = decodeExampleRoute(exampleHash(ex))!;
    expect(p.files[0]).not.toBe(ex.files[0]);
    p.files[0].content = "mutated";
    expect(ex.files[0].content).not.toBe("mutated");
  });

  it("looks up slugs case-insensitively", () => {
    const ex = EXAMPLES[0];
    expect(findExampleBySlug(ex.slug.toUpperCase())).toBe(ex);
  });

  it("returns null for non-routes and unknown slugs", () => {
    expect(decodeExampleRoute("#code/whatever")).toBeNull();
    expect(decodeExampleRoute("")).toBeNull();
    expect(decodeExampleRoute(`${EXAMPLE_PREFIX}does-not-exist`)).toBeNull();
  });
});
