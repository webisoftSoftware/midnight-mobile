import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  findSecretErrors,
  validateCargoDependencies,
  validateNpmDependencies,
  validateSecurityPolicy,
} from "./security-gates.mjs";

const policy = JSON.parse(
  readFileSync(new URL("./security-policy.json", import.meta.url), "utf8"),
);

function npmFixture(version = "1.1.17") {
  return {
    packages: {
      "node_modules/brace-expansion": {
        version,
        resolved: `https://registry.npmjs.org/brace-expansion/-/brace-expansion-${version}.tgz`,
        integrity: "sha512-synthetic",
        license: "MIT",
      },
      "node_modules/uuid": {
        version: "11.1.1",
        resolved: "https://registry.npmjs.org/uuid/-/uuid-11.1.1.tgz",
        integrity: "sha512-synthetic",
        license: "MIT",
      },
    },
  };
}

const overrides = {
  "brace-expansion@<2": "1.1.17",
  "brace-expansion@>=2 <3": "2.1.3",
  "brace-expansion@>=5 <6": "5.0.8",
  uuid: "11.1.1",
};

await test("security policy fixes M5 advisory handling", () => {
  assert.deepEqual(validateSecurityPolicy(policy), []);
  assert.match(
    validateSecurityPolicy({
      ...policy,
      currentMilestone: "M4",
    }).join("\n"),
    /currentMilestone/u,
  );
  assert.match(
    validateSecurityPolicy({
      ...policy,
      rustSecInformationalAllowlist: [
        ...policy.rustSecInformationalAllowlist,
        {
          advisory: "RUSTSEC-synthetic",
          package: "synthetic",
          version: "0.0.0",
          kind: "unmaintained",
          trackingIssue: "https://example.invalid",
          removalCondition: "synthetic",
        },
      ],
    }).join("\n"),
    /rustSecInformationalAllowlist/u,
  );
});

await test("secret scanning recognizes supported credential families", () => {
  const candidates = [
    ["aws", ["AK", "IA", "A".repeat(16)].join("")],
    ["github", ["gh", "p_", "b".repeat(24)].join("")],
    ["private", ["-----BEGIN ", "PRIVATE KEY-----"].join("")],
  ];
  assert.equal(
    findSecretErrors(candidates.map(([path, contents]) => ({ path, contents })))
      .length,
    candidates.length,
  );
  assert.deepEqual(
    findSecretErrors([{ path: "safe", contents: "synthetic-key-material" }]),
    [],
  );
});

await test("npm dependency gate requires patched brace evidence", () => {
  const lockfile = npmFixture();
  lockfile.packages["node_modules/a/brace-expansion"] = {
    ...lockfile.packages["node_modules/brace-expansion"],
    version: "2.1.3",
  };
  lockfile.packages["node_modules/b/brace-expansion"] = {
    ...lockfile.packages["node_modules/brace-expansion"],
    version: "5.0.8",
  };
  const evidence = new Map(
    Object.keys(lockfile.packages).map((path) => [
      path,
      `${policy.braceExpansionAdvisory.cve} 4_000_000`,
    ]),
  );
  assert.deepEqual(
    validateNpmDependencies(
      { overrides, devDependencies: { xcode: "3.0.1" } },
      lockfile,
      evidence,
      policy,
    ),
    [],
  );
  evidence.set("node_modules/brace-expansion", "");
  assert.match(
    validateNpmDependencies(
      { overrides, devDependencies: { xcode: "3.0.1" } },
      lockfile,
      evidence,
      policy,
    ).join("\n"),
    /patched expansion-length bound/u,
  );
});

await test("npm dependency gate rejects the vulnerable workspace uuid tree", () => {
  const lockfile = npmFixture();
  lockfile.packages["node_modules/a/brace-expansion"] = {
    ...lockfile.packages["node_modules/brace-expansion"],
    version: "2.1.3",
  };
  lockfile.packages["node_modules/b/brace-expansion"] = {
    ...lockfile.packages["node_modules/brace-expansion"],
    version: "5.0.8",
  };
  lockfile.packages["node_modules/uuid"].version = "7.0.3";
  const evidence = new Map(
    Object.keys(lockfile.packages)
      .filter((path) => path.endsWith("/brace-expansion"))
      .map((path) => [path, `${policy.braceExpansionAdvisory.cve} 4_000_000`]),
  );
  const errors = validateNpmDependencies(
    { overrides, devDependencies: {} },
    lockfile,
    evidence,
    policy,
  );
  assert.ok(errors.some((error) => error.includes("xcode security pin")));
  assert.ok(errors.some((error) => error.includes("uuid versions")));
});

await test("Cargo dependency gate rejects missing licenses and Git drift", () => {
  const valid = {
    packages: [
      {
        name: "ledger",
        version: "1.0.0",
        license: "Apache-2.0",
        license_file: null,
        source: `git+https://example.invalid/repo?rev=${policy.ledgerGitRevision}`,
      },
    ],
  };
  assert.deepEqual(validateCargoDependencies(valid, policy), []);
  assert.equal(
    validateCargoDependencies(
      {
        packages: [
          { ...valid.packages[0], license: null, source: "git+https://bad" },
        ],
      },
      policy,
    ).length,
    2,
  );
});
