import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

const script = resolve(import.meta.dirname, "validate-glb-corpus.mjs");

const fixture = async () => {
  const root = await mkdtemp(resolve(tmpdir(), "openrscad-glb-corpus-test-"));
  await writeFile(resolve(root, "main.scad"), "cube(1);");
  const validator = resolve(root, "validator.mjs");
  await writeFile(
    validator,
    `import { appendFileSync } from "node:fs";
export const version = "test-validator";
export const validateBytes = async () => {
  if (process.env.MOCK_CALL_LOG) appendFileSync(process.env.MOCK_CALL_LOG, "call\\n");
  const severity = Number(process.env.MOCK_SEVERITY ?? -1);
  const messages = severity < 0 ? [] : [{ severity, code: "MOCK", pointer: "/meshes/0", message: "mock issue" }];
  return { validatorVersion: version, issues: { numErrors: severity === 0 ? 1 : 0, numWarnings: severity === 1 ? 1 : 0, messages } };
};
`,
  );
  return { root, validator };
};

const run = ({ root, validator }, patterns, report, extraEnv = {}, budgets) =>
  spawnSync(
    process.execPath,
    [
      script,
      "--root",
      root,
      "--report",
      report,
      ...(budgets ? ["--budgets", budgets] : []),
      ...patterns,
    ],
    {
      encoding: "utf8",
      env: {
        ...process.env,
        OPENRSCAD_GLTF_VALIDATOR_MODULE: validator,
        ...extraEnv,
      },
    },
  );

const withBudgets = async (root, budgets) => {
  const path = resolve(root, "budgets.json");
  await writeFile(path, JSON.stringify(budgets));
  return path;
};

test("overlapping globs are sorted, deduplicated, and validated in both edge modes", async () => {
  const files = await fixture();
  await writeFile(resolve(files.root, "z.scad"), "sphere(1);");
  const report = resolve(files.root, "report.json");
  const calls = resolve(files.root, "calls.txt");
  const result = run(files, ["*.scad", "main.*"], report, { MOCK_CALL_LOG: calls });

  assert.equal(result.status, 0, result.stderr);
  const parsed = JSON.parse(await readFile(report, "utf8"));
  assert.deepEqual(
    parsed.results.map(({ source, includeEdges }) => [source, includeEdges]),
    [
      ["main.scad", false],
      ["main.scad", true],
      ["z.scad", false],
      ["z.scad", true],
    ],
  );
  assert.equal((await readFile(calls, "utf8")).trim().split("\n").length, 4);
  assert.equal(parsed.failed, 0);
  assert.ok(parsed.results.every(({ deterministic }) => deterministic));
});

test("validator errors and warnings are both fatal and retained", async () => {
  for (const severity of [0, 1]) {
    const files = await fixture();
    const report = resolve(files.root, `report-${severity}.json`);
    const result = run(files, ["*.scad"], report, { MOCK_SEVERITY: String(severity) });

    assert.equal(result.status, 1);
    const parsed = JSON.parse(await readFile(report, "utf8"));
    assert.equal(parsed.failed, 2);
    assert.equal(parsed.results[0].validatorMessages[0].code, "MOCK");
    assert.equal(parsed.results[0].validatorMessages[0].pointer, "/meshes/0");
  }
});

test("empty matches and export failures are nonzero without hiding valid entries", async () => {
  const empty = await fixture();
  const emptyResult = run(empty, ["missing/*.scad"], resolve(empty.root, "empty.json"));
  assert.equal(emptyResult.status, 1);

  const files = await fixture();
  await writeFile(resolve(files.root, "invalid.scad"), "cube(;");
  const report = resolve(files.root, "report.json");
  const result = run(files, ["*.scad"], report);
  assert.equal(result.status, 1);
  const parsed = JSON.parse(await readFile(report, "utf8"));
  assert.equal(parsed.failed, 2);
  assert.equal(parsed.passed, 2);
  assert.ok(parsed.results.some(({ source, ok }) => source === "main.scad" && ok));
});

test("a line-segment budget is a ceiling, and only the edged export is measured", async () => {
  const files = await fixture();
  const report = resolve(files.root, "report.json");
  // `cube(1);` draws its twelve edges; the plain export draws none, so a budget
  // of zero must not fail it.
  const budgets = await withBudgets(files.root, { "main.scad": { lineSegments: 12 } });
  const passing = run(files, ["main.scad"], report, {}, budgets);

  assert.equal(passing.status, 0, passing.stderr);
  const parsed = JSON.parse(await readFile(report, "utf8"));
  assert.equal(parsed.results[1].lineCount, 12);
  assert.deepEqual(parsed.results[1].budget, { lineSegments: 12 });
  assert.equal(parsed.results[0].budget, undefined);

  const tightened = await withBudgets(files.root, { "main.scad": { lineSegments: 11 } });
  const failing = run(files, ["main.scad"], resolve(files.root, "tight.json"), {}, tightened);
  assert.equal(failing.status, 1);
  assert.match(failing.stderr, /12 line segments exceeds the budget of 11/);
});

test("models with no budget entry are still held to seam closure", async () => {
  const files = await fixture();
  const report = resolve(files.root, "report.json");
  const budgets = await withBudgets(files.root, { "other.scad": { lineSegments: 1 } });
  const result = run(files, ["main.scad"], report, {}, budgets);

  assert.equal(result.status, 0, result.stderr);
  const parsed = JSON.parse(await readFile(report, "utf8"));
  assert.equal(parsed.budgets, budgets);
  assert.equal(parsed.results[1].danglingEndpointCount, 0);
  assert.equal(parsed.results[1].budget, undefined);
});

test("an unreadable budget file stops the run rather than silently gating nothing", async () => {
  const files = await fixture();
  const result = run(
    files,
    ["main.scad"],
    resolve(files.root, "report.json"),
    {},
    resolve(files.root, "missing-budgets.json"),
  );

  assert.equal(result.status, 1);
  assert.match(result.stderr, /unreadable budget file/);
});
