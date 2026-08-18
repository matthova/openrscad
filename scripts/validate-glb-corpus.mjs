#!/usr/bin/env node
// Export a globbed SCAD corpus through the built Node Wasm facade and validate
// every GLB, with and without feature edges, using Khronos glTF Validator.
//
// Usage: node scripts/validate-glb-corpus.mjs --root DIR [--report FILE]
//          [--budgets FILE] GLOB...
// Exit: 0 all valid; 1 bad input/export/validation/budget; 3 missing build/dependency.

import { mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { relative, resolve } from "node:path";
import process from "node:process";
import { pathToFileURL } from "node:url";

import { discoverCorpus, posix, requestAssets } from "./lib/corpus.mjs";
import { glbCounts } from "./lib/glb.mjs";

const parseArgs = (args) => {
  let root = process.cwd();
  let report;
  let budgets;
  const patterns = [];
  for (let index = 0; index < args.length; index += 1) {
    if (args[index] === "--root") root = args[++index];
    else if (args[index] === "--report") report = args[++index];
    else if (args[index] === "--budgets") budgets = args[++index];
    else patterns.push(args[index]);
  }
  if (!root || patterns.length === 0 || patterns.some((pattern) => !pattern)) return null;
  if (budgets === "") return null;
  return {
    budgets: budgets && resolve(budgets),
    root: resolve(root),
    report: report && resolve(report),
    patterns,
  };
};

const issueSeverity = ["error", "warning", "info", "hint"];

const validatorModule = async () => {
  const override = process.env.OPENRSCAD_GLTF_VALIDATOR_MODULE;
  return import(override ? pathToFileURL(resolve(override)).href : "gltf-validator");
};

const main = async () => {
  const options = parseArgs(process.argv.slice(2));
  if (!options) {
    console.error(
      "Usage: node scripts/validate-glb-corpus.mjs --root DIR [--report FILE] GLOB...",
    );
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

  let budgets = {};
  if (options.budgets) {
    try {
      budgets = JSON.parse(await readFile(options.budgets, "utf8"));
    } catch (error) {
      console.error(`ERROR: unreadable budget file ${options.budgets}: ${error.message}`);
      process.exit(1);
    }
  }

  let exportShape3D;
  let validateBytes;
  let declaredValidatorVersion;
  try {
    ({ exportShape3D } = await import(
      resolve(import.meta.dirname, "../packages/npm/dist/node.js")
    ));
    ({ validateBytes, version: declaredValidatorVersion } = await validatorModule());
  } catch (error) {
    console.error(`ERROR: build packages/npm and install gltf-validator: ${error.message}`);
    process.exit(3);
  }

  const outputDirectory = await mkdtemp(resolve(tmpdir(), "openrscad-glb-"));
  const results = [];
  let validatorVersion = declaredValidatorVersion ?? "unknown";
  for (const [index, entry] of entries.entries()) {
    const source = await readFile(entry, "utf8");
    const sourceName = posix(relative(options.root, entry));
    const request = await requestAssets(entry, assets);
    for (const includeEdges of [false, true]) {
      const started = performance.now();
      const record = { includeEdges, ok: false, source: sourceName };
      const artifact = resolve(
        outputDirectory,
        `${String(index).padStart(5, "0")}-${includeEdges ? "edges" : "plain"}.glb`,
      );
      try {
        const exported = await exportShape3D(source, "glb", { ...request, includeEdges });
        Object.assign(record, {
          diagnostics: exported.diagnostics,
          engineWarnings: exported.warnings,
          geomErrors: exported.geomErrors,
        });
        if (!exported.ok) throw new Error(exported.error);
        if (exported.geomErrors) throw new Error(`geometry errors: ${exported.geomErrors}`);
        await writeFile(artifact, exported.bytes);

        const repeated = await exportShape3D(source, "glb", { ...request, includeEdges });
        if (!repeated.ok) throw new Error(`repeat export failed: ${repeated.error}`);
        const deterministic = Buffer.from(exported.bytes).equals(Buffer.from(repeated.bytes));
        record.deterministic = deterministic;
        if (!deterministic) throw new Error("repeated export produced different bytes");

        const validation = await validateBytes(exported.bytes, { uri: sourceName });
        validatorVersion = validation.validatorVersion ?? validatorVersion;
        const validatorMessages = validation.issues?.messages ?? [];
        Object.assign(record, {
          artifactBytes: exported.bytes.length,
          ...glbCounts(exported.bytes),
          validatorErrors: validation.issues?.numErrors ?? 0,
          validatorMessages,
          validatorWarnings: validation.issues?.numWarnings ?? 0,
        });
        for (const issue of validatorMessages) {
          console.error(
            `${issueSeverity[issue.severity] ?? issue.severity} ${issue.code} ${issue.pointer || "/"}: ${issue.message}`,
          );
        }
        if (record.validatorErrors || record.validatorWarnings) {
          throw new Error(
            `validator reported ${record.validatorErrors} error(s) and ${record.validatorWarnings} warning(s)`,
          );
        }

        // Two halves of one gate. The ceiling catches a classifier that stops
        // recognising a surface and falls back to drawing every triangle; closure
        // catches the opposite failure, a seam that merges away in the middle and
        // leaves loose ends behind. Closure is checked everywhere, because no
        // model in the corpus or the fixture set has a single dangling endpoint;
        // a budget entry can raise the allowance if one ever legitimately does.
        if (includeEdges) {
          const budget = budgets[sourceName];
          if (budget) record.budget = budget;
          if (budget?.lineSegments !== undefined && record.lineCount > budget.lineSegments) {
            throw new Error(
              `${record.lineCount} line segments exceeds the budget of ${budget.lineSegments}`,
            );
          }
          const allowedEndpoints = budget?.danglingEndpoints ?? 0;
          if (record.danglingEndpointCount > allowedEndpoints) {
            throw new Error(
              `${record.danglingEndpointCount} dangling seam endpoints exceeds the budget of ${allowedEndpoints}`,
            );
          }
        }
        record.ok = true;
        console.log(`✓ ${sourceName} (${includeEdges ? "edges" : "plain"})`);
      } catch (error) {
        record.error = error instanceof Error ? error.message : String(error);
        console.error(
          `ERROR: ${sourceName} (${includeEdges ? "edges" : "plain"}): ${record.error}`,
        );
      }
      record.durationMs = performance.now() - started;
      results.push(record);
    }
  }

  const failed = results.filter((result) => !result.ok).length;
  const report = {
    assetRoot,
    budgets: options.budgets ?? null,
    corpusRoot: options.root,
    failed,
    matchedEntries: entries.length,
    passed: results.length - failed,
    patterns: options.patterns,
    results,
    validator: validatorVersion,
  };
  if (options.report) await writeFile(options.report, `${JSON.stringify(report, null, 2)}\n`);
  if (failed === 0) {
    await rm(outputDirectory, { recursive: true, force: true });
    console.log(`✓ ${entries.length} GLB exports passed validation in both edge modes`);
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
