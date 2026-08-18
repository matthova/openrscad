import assert from "node:assert/strict";
import { chmod, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

const script = resolve(import.meta.dirname, "validate-3mf-corpus.mjs");

const fixture = async (validatorExit = 0) => {
  const root = await mkdtemp(resolve(tmpdir(), "openrscad-corpus-test-"));
  await writeFile(resolve(root, "main.scad"), "cube(1);");
  const validator = resolve(root, "validator.mjs");
  await writeFile(
    validator,
    `#!/usr/bin/env node\nif (process.argv[2] === "--version") console.log("3mf 0.4.0");\nelse { console.log(JSON.stringify({ operations: { validate: { passed: ${validatorExit === 0}, errors: [], warnings: [], info: [], level: "paranoid" } } })); process.exit(${validatorExit}); }\n`,
  );
  await chmod(validator, 0o755);
  return { root, validator };
};

const run = ({ root, validator }, patterns, report) =>
  spawnSync(
    process.execPath,
    [script, "--root", root, "--report", report, ...patterns],
    {
      encoding: "utf8",
      env: { ...process.env, OPENRSCAD_3MF_VALIDATOR: validator },
    },
  );

test("overlapping globs are sorted and deduplicated", async () => {
  const files = await fixture();
  await writeFile(resolve(files.root, "z.scad"), "sphere(1);");
  await writeFile(resolve(files.root, "a.scad"), "cylinder(1, 1);");
  const report = resolve(files.root, "report.json");
  const result = run(files, ["*.scad", "main.*"], report);
  assert.equal(result.status, 0, result.stderr);
  const parsed = JSON.parse(await readFile(report, "utf8"));
  assert.deepEqual(
    parsed.results.map(({ source }) => source),
    ["a.scad", "main.scad", "z.scad"],
  );
  assert.equal(parsed.failed, 0);
});

test("empty matches and validator failures are nonzero", async () => {
  const empty = await fixture();
  const emptyResult = run(empty, ["missing/*.scad"], resolve(empty.root, "empty.json"));
  assert.equal(emptyResult.status, 1);

  const invalid = await fixture(1);
  const report = resolve(invalid.root, "report.json");
  const invalidResult = run(invalid, ["*.scad"], report);
  assert.equal(invalidResult.status, 1);
  const parsed = JSON.parse(await readFile(report, "utf8"));
  assert.equal(parsed.failed, 1);
  assert.equal(parsed.results[0].validatorStatus, 1);
});

test("validator warnings are reported and fatal", async () => {
  const files = await fixture();
  await writeFile(
    files.validator,
    `#!/usr/bin/env node\nif (process.argv[2] === "--version") console.log("3mf 0.4.0");\nelse console.log(JSON.stringify({ operations: { validate: { passed: true, errors: [], warnings: [{ code: 42, message: "suspicious mesh" }], info: [], level: "paranoid" } } }));\n`,
  );
  const report = resolve(files.root, "report.json");
  const result = run(files, ["*.scad"], report);

  assert.equal(result.status, 1);
  const parsed = JSON.parse(await readFile(report, "utf8"));
  assert.deepEqual(parsed.results[0].validatorWarnings, [
    { code: 42, message: "suspicious mesh" },
  ]);
  assert.match(result.stderr, /WARN 42.*suspicious mesh/);
});

test("validator stderr and structured errors are emitted and retained", async () => {
  const files = await fixture();
  await writeFile(
    files.validator,
    `#!/usr/bin/env node\nif (process.argv[2] === "--version") console.log("3mf 0.4.0");\nelse { console.error("validator diagnostic on stderr"); console.log(JSON.stringify({ operations: { validate: { passed: false, errors: [{ code: 7, message: "broken package" }], warnings: [], info: [], level: "paranoid" } } })); process.exit(1); }\n`,
  );
  const report = resolve(files.root, "report.json");
  const result = run(files, ["*.scad"], report);

  assert.equal(result.status, 1);
  assert.match(result.stderr, /validator diagnostic on stderr/);
  assert.match(result.stderr, /ERROR 7.*broken package/);
  const parsed = JSON.parse(await readFile(report, "utf8"));
  assert.deepEqual(parsed.results[0].validatorErrors, [
    { code: 7, message: "broken package" },
  ]);
  assert.equal(parsed.results[0].stderr.trim(), "validator diagnostic on stderr");
});

test("export failures are retained while valid entries still validate", async () => {
  const files = await fixture();
  await writeFile(resolve(files.root, "invalid.scad"), "cube(;");
  const report = resolve(files.root, "report.json");
  const result = run(files, ["*.scad"], report);
  assert.equal(result.status, 1);
  const parsed = JSON.parse(await readFile(report, "utf8"));
  assert.equal(parsed.failed, 1);
  assert.equal(parsed.passed, 1);
  assert.equal(parsed.results[0].error.includes("parse error"), true);
  assert.equal(parsed.results[1].ok, true);
});
