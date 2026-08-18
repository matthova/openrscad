#!/usr/bin/env node
// Export a globbed SCAD corpus through the built Node Wasm facade and validate
// every 3MF at lib3mf-cli's maximum (paranoid) level.
//
// Required tools: a built packages/npm artifact and lib3mf-cli 0.4.0 (`3mf`).
// Optional env: OPENRSCAD_3MF_VALIDATOR overrides the validator executable.
// Usage: node scripts/validate-3mf-corpus.mjs --root DIR [--report FILE] GLOB...
// Exit: 0 all valid; 1 bad input/export/validation; 3 missing dependency.

import { mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { relative, resolve } from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

import { discoverCorpus, posix, requestAssets } from "./lib/corpus.mjs";

const usage = () => {
  console.error(
    "Usage: node scripts/validate-3mf-corpus.mjs --root DIR [--report FILE] GLOB...",
  );
};

const parseArgs = (args) => {
  let root = process.cwd();
  let report;
  const patterns = [];
  for (let index = 0; index < args.length; index += 1) {
    if (args[index] === "--root") {
      root = args[++index];
    } else if (args[index] === "--report") {
      report = args[++index];
    } else {
      patterns.push(args[index]);
    }
  }
  if (!root || patterns.length === 0 || patterns.some((pattern) => !pattern)) {
    return null;
  }
  return { root: resolve(root), report: report && resolve(report), patterns };
};

const validationReport = (stdout) => {
  const records = stdout
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line));
  const validation = records[0]?.operations?.validate;
  if (!validation) throw new Error("validator returned no structured validation report");
  return validation;
};

const main = async () => {
  const options = parseArgs(process.argv.slice(2));
  if (!options) {
    usage();
    process.exit(1);
  }
  try {
    if (!(await stat(options.root)).isDirectory()) throw new Error("root is not a directory");
  } catch (error) {
    console.error(`ERROR: invalid corpus root ${options.root}: ${error.message}`);
    process.exit(1);
  }

  const corpus = await discoverCorpus(options);
  if (!corpus) {
    console.error("ERROR: the supplied patterns matched no files");
    process.exit(1);
  }
  const { assetRoot, assets, entries } = corpus;
  const engineEntry = resolve(import.meta.dirname, "../packages/npm/dist/node.js");
  let exportShape3D;
  try {
    ({ exportShape3D } = await import(engineEntry));
  } catch (error) {
    console.error(`ERROR: build packages/npm before validation: ${error.message}`);
    process.exit(3);
  }
  const validator = process.env.OPENRSCAD_3MF_VALIDATOR || "3mf";
  const version = spawnSync(validator, ["--version"], { encoding: "utf8" });
  if (version.error) {
    console.error(`ERROR: cannot run ${validator}: ${version.error.message}`);
    process.exit(3);
  }

  const outputDirectory = await mkdtemp(resolve(tmpdir(), "openrscad-3mf-"));
  const results = [];
  for (const [index, entry] of entries.entries()) {
    const started = performance.now();
    const record = { source: posix(relative(options.root, entry)), ok: false };
    try {
      const source = await readFile(entry, "utf8");
      const exported = await exportShape3D(source, "3mf", await requestAssets(entry, assets));
      Object.assign(record, {
        diagnostics: exported.diagnostics,
        engineWarnings: exported.warnings,
        geomErrors: exported.geomErrors,
      });
      if (!exported.ok) throw new Error(exported.error);
      if (exported.geomErrors) throw new Error(`geometry errors: ${exported.geomErrors}`);
      const artifact = resolve(outputDirectory, `${String(index).padStart(5, "0")}.3mf`);
      await writeFile(artifact, exported.bytes);
      const validation = spawnSync(
        validator,
        [
          "batch",
          artifact,
          "--validate",
          "--validate-level",
          "paranoid",
          "--format",
          "json",
        ],
        { encoding: "utf8" },
      );
      if (validation.error) throw validation.error;
      const validatorReport = validationReport(validation.stdout);
      Object.assign(record, {
        artifactBytes: exported.bytes.length,
        durationMs: performance.now() - started,
        stderr: validation.stderr,
        stdout: validation.stdout,
        validatorErrors: validatorReport.errors,
        validatorStatus: validation.status,
        validatorWarnings: validatorReport.warnings,
      });
      if (validation.stderr) console.error(validation.stderr.trimEnd());
      for (const error of validatorReport.errors) {
        console.error(`ERROR ${error.code}: ${error.message}`);
      }
      for (const warning of validatorReport.warnings) {
        console.error(`WARN ${warning.code}: ${warning.message}`);
      }
      if (validation.status !== 0) throw new Error(`validator exited ${validation.status}`);
      if (validatorReport.errors.length || validatorReport.warnings.length) {
        throw new Error(
          `validator reported ${validatorReport.errors.length} error(s) and ${validatorReport.warnings.length} warning(s)`,
        );
      }
      record.ok = true;
      console.log(`✓ ${record.source}`);
    } catch (error) {
      record.durationMs = performance.now() - started;
      record.error = error instanceof Error ? error.message : String(error);
      console.error(`ERROR: ${record.source}: ${record.error}`);
    }
    results.push(record);
  }

  const failed = results.filter((result) => !result.ok).length;
  const report = {
    assetRoot,
    corpusRoot: options.root,
    failed,
    matchedEntries: entries.length,
    passed: results.length - failed,
    patterns: options.patterns,
    results,
    validator: version.stdout.trim() || version.stderr.trim(),
  };
  if (options.report) await writeFile(options.report, `${JSON.stringify(report, null, 2)}\n`);
  if (failed === 0) {
    await rm(outputDirectory, { recursive: true, force: true });
    console.log(`✓ ${results.length} 3MF exports passed paranoid validation`);
    return;
  }
  console.error(`ERROR: ${failed}/${results.length} failed; artifacts retained at ${outputDirectory}`);
  process.exit(1);
};

try {
  await main();
} catch (error) {
  console.error("ERROR:", error);
  process.exit(1);
}
