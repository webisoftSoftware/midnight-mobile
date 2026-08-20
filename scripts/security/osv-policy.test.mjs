import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { validateOsvReport } from "./osv-policy.mjs";

const policy = JSON.parse(
  readFileSync(new URL("./security-policy.json", import.meta.url), "utf8"),
);

function vulnerability() {
  return {
    id: "GHSA-mh99-v99m-4gvg",
    aliases: ["CVE-2026-14257"],
  };
}

function affected(name, version, vulnerabilities = [vulnerability()]) {
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

await test("OSV accepts only exact brace findings on reviewed patched nodes", () => {
  assert.deepEqual(
    validateOsvReport(
      report([
        affected("brace-expansion", "1.1.17"),
        affected("brace-expansion", "5.0.8"),
      ]),
      policy,
    ),
    [],
  );
});

await test("mixed OSV findings and unverified patched sources fail", () => {
  const errors = validateOsvReport(
    report([
      affected("brace-expansion", "1.1.17"),
      affected("brace-expansion", "2.1.3"),
      affected("postcss", "8.5.1", [
        { id: "GHSA-other", aliases: ["CVE-other"] },
      ]),
    ]),
    policy,
    ["node_modules/brace-expansion: patched source evidence is missing"],
  );
  assert.ok(errors.some((error) => error.includes("unapproved OSV finding")));
  assert.ok(errors.some((error) => error.includes("source evidence")));
});

await test("OSV schema and advisory aliases fail closed", () => {
  assert.match(validateOsvReport({}, policy).join("\n"), /results array/u);
  assert.match(
    validateOsvReport(
      report([
        affected("brace-expansion", "1.1.17", [
          {
            ...vulnerability(),
            aliases: ["CVE-2026-14257", "GHSA-other"],
          },
        ]),
      ]),
      policy,
    ).join("\n"),
    /unapproved OSV finding/u,
  );
});
