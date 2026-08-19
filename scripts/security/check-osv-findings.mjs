import { readFileSync, readdirSync, statSync } from "node:fs";
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

function collectJavaScript(path, output = []) {
  for (const entry of readdirSync(path).sort()) {
    const candidate = join(path, entry);
    if (statSync(candidate).isDirectory()) collectJavaScript(candidate, output);
    else if (entry.endsWith(".js"))
      output.push(readFileSync(candidate, "utf8"));
  }
  return output;
}

function braceEvidence(lockfile) {
  const evidence = new Map();
  for (const path of Object.keys(lockfile.packages ?? {})) {
    if (!path.endsWith("/brace-expansion")) continue;
    evidence.set(
      path,
      collectJavaScript(join(REPOSITORY_ROOT, path)).join("\n"),
    );
  }
  return evidence;
}

export function checkOsvFindings(arguments_ = process.argv.slice(2)) {
  if (arguments_.length !== 1) fail("expected one OSV JSON path");
  const policy = JSON.parse(
    readFileSync(
      join(REPOSITORY_ROOT, "scripts/security/security-policy.json"),
      "utf8",
    ),
  );
  const packageMetadata = JSON.parse(
    readFileSync(join(REPOSITORY_ROOT, "package.json"), "utf8"),
  );
  const lockfile = JSON.parse(
    readFileSync(join(REPOSITORY_ROOT, "package-lock.json"), "utf8"),
  );
  const report = JSON.parse(readFileSync(resolve(arguments_[0]), "utf8"));
  const errors = validateOsvReport(
    report,
    policy,
    validateNpmDependencies(
      packageMetadata,
      lockfile,
      braceEvidence(lockfile),
      policy,
    ),
  );
  if (errors.length > 0) fail(errors.sort().join("\n"));
  const affected = report.results.reduce(
    (count, result) => count + result.packages.length,
    0,
  );
  console.log(
    `OSV finding gate passed: affected=${String(
      affected,
    )}, exact-reviewed-advisory=${policy.braceExpansionAdvisory.advisory}`,
  );
}

checkOsvFindings();
