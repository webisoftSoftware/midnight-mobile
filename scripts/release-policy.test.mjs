import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  cargoComponents,
  npmComponents,
  validateDistributionDecision,
  validateReleaseConfig,
  validateReleaseTag,
} from "./release-policy.mjs";

const config = JSON.parse(
  readFileSync(new URL("./release-config.json", import.meta.url), "utf8"),
);

function inputs() {
  return {
    rootMetadata: { packageManager: "npm@11.12.1" },
    packageMetadata: {
      name: "@1am/midnight-mobile",
      version: "0.1.0-alpha.1",
    },
    nativeConfig: {
      rust: { toolchain: "1.97.1" },
      android: { ndkVersion: "27.1.12297006" },
      apple: { deploymentTarget: "15.1" },
    },
    cargoLock: config.compatibility.midnightLedgerRevision,
    compatibilityDocument: Object.values(config.compatibility)
      .flat()
      .join("\n"),
  };
}

await test("release config pins package, compatibility, and toolchains", () => {
  assert.deepEqual(validateReleaseConfig(config, inputs()), []);
  assert.match(
    validateReleaseConfig(
      { ...config, releaseEnvironment: "automatic" },
      inputs(),
    ).join("\n"),
    /releaseEnvironment/u,
  );
});

await test("release tag must match and point at the commit", () => {
  assert.deepEqual(
    validateReleaseTag(config.package.gitTag, [config.package.gitTag], config),
    [],
  );
  assert.equal(validateReleaseTag("v0.1.0", [], config).length, 2);
});

await test("public distribution requires a reviewed M6 decision", () => {
  const decision = {
    schemaVersion: 1,
    status: "approved",
    packageVersion: config.package.version,
    copyrightOwner: "Reviewed owner",
    sourceLicenseExpression: "Reviewed expression",
    packageLicenseExpression: "Reviewed expression",
    reviewedBy: "Maintainer",
    reviewedAt: "2026-08-01",
  };
  assert.deepEqual(validateDistributionDecision(decision, config), []);
  assert.match(
    validateDistributionDecision(
      { ...decision, status: "deferred" },
      config,
    ).join("\n"),
    /distribution status/u,
  );
});

await test("SBOM component builders are sorted and path-neutral", () => {
  assert.deepEqual(
    npmComponents({
      packages: {
        "node_modules/b": {
          version: "2.0.0",
          license: "MIT",
          resolved: "registry",
          integrity: "sha512-b",
        },
        "node_modules/a": {
          version: "1.0.0",
          license: "MIT",
          resolved: "registry",
          integrity: "sha512-a",
        },
      },
    }).map((component) => component.name),
    ["a", "b"],
  );
  assert.deepEqual(
    cargoComponents({
      packages: [
        {
          name: "b",
          version: "2.0.0",
          license: "MIT",
          source: null,
        },
        {
          name: "a",
          version: "1.0.0",
          license: "Apache-2.0",
          source: "registry",
        },
      ],
    }).map((component) => component.name),
    ["a", "b"],
  );
});
