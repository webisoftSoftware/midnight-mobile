import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { validateOsvReport } from "./osv-policy.mjs";
import { validateNpmDependencies } from "./security-gates.mjs";

const REPOSITORY_ROOT = resolve(
  dirname(fileURLToPath(import.meta.url)),
  "../..",
);

function fail(message) {
  throw new Error(`OSV finding gate: ${message}`);
}

export function checkOsvFindings(arguments_ = process.argv.slice(2)) {
  if (arguments_.length !== 1) fail("expected one OSV JSON path");
  const packageMetadata = JSON.parse(
    readFileSync(join(REPOSITORY_ROOT, "package.json"), "utf8"),
  );
  const lockfile = JSON.parse(
    readFileSync(join(REPOSITORY_ROOT, "package-lock.json"), "utf8"),
  );
  const report = JSON.parse(readFileSync(resolve(arguments_[0]), "utf8"));
  const errors = validateOsvReport(
    report,
    validateNpmDependencies(packageMetadata, lockfile),
  );
  if (errors.length > 0) fail(errors.sort().join("\n"));
  const affected = report.results.reduce(
    (count, result) => count + result.packages.length,
    0,
  );
  console.log(`OSV finding gate passed: affected=${String(affected)}`);
}

checkOsvFindings();
