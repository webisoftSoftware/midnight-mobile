import assert from "node:assert/strict";
import test from "node:test";

import { validateOsvReport } from "./osv-policy.mjs";

function affected(name, version, vulnerabilities) {
  return {
    package: { name, version, ecosystem: "npm" },
    vulnerabilities,
  };
}

function report(packages) {
  return {
    results: [
      {
        source: { path: "package-lock.json", type: "lockfile" },
        packages,
      },
    ],
  };
}

await test("OSV accepts a report with no findings", () => {
  assert.deepEqual(validateOsvReport(report([])), []);
});

await test("OSV findings and dependency verification errors fail", () => {
  const errors = validateOsvReport(
    report([
      affected("postcss", "8.5.1", [
        { id: "GHSA-other", aliases: ["CVE-other"] },
      ]),
    ]),
    ["node_modules/example: dependency verification failed"],
  );
  assert.ok(errors.some((error) => error.includes("OSV finding")));
  assert.ok(errors.some((error) => error.includes("verification failed")));
});

await test("OSV schema and malformed findings fail closed", () => {
  assert.match(validateOsvReport({}).join("\n"), /results array/u);
  assert.match(
    validateOsvReport(
      report([affected("brace-expansion", "1.1.18", [null])]),
    ).join("\n"),
    /vulnerability entry is invalid/u,
  );
});
