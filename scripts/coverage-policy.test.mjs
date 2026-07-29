import assert from "node:assert/strict";
import test from "node:test";

import {
  RUST_M2_ENFORCED,
  RUST_M2_MEASURED,
  RUST_TARGET,
  TYPESCRIPT_TARGET,
  loadCoveragePolicy,
  validateCoverageManifest,
} from "./coverage-policy.mjs";

function manifest() {
  return {
    currentMilestone: "M2",
    rustCoverageException: {
      rule: "rust-line-coverage",
      minimumPercent: RUST_TARGET,
      measuredPercent: RUST_M2_MEASURED,
      enforcedPercent: RUST_M2_ENFORCED,
      trackingIssue: "https://github.com/ADGLx/midnight-mobile/issues/40",
      activeFromMilestone: "M1",
      expiresAtMilestone: "M5",
      removalCondition:
        "Raise measured Rust runtime line coverage to the required target before M5 begins.",
    },
    typescriptCoverage: {
      rule: "typescript-coverage",
      runner: "compiled-production-node-tests",
      include: "packages/react-native/.staging-build/src/**/*.js",
      setupImport: "packages/react-native/test-support/register-peer-stubs.mjs",
      minimumPercent: { ...TYPESCRIPT_TARGET },
    },
  };
}

await test("accepts the permanent TypeScript gate and active Rust exception", () => {
  const policy = validateCoverageManifest(manifest());
  assert.equal(policy.currentMilestone, "M2");
  assert.deepEqual(policy.typescript.minimumPercent, TYPESCRIPT_TARGET);
  assert.equal(policy.rust.measuredPercent, RUST_M2_MEASURED);
  assert.equal(policy.rust.enforcedPercent, RUST_M2_ENFORCED);
});

await test("rejects a TypeScript threshold reduction", () => {
  const value = manifest();
  value.typescriptCoverage.minimumPercent.lines = 70;
  assert.throws(() => validateCoverageManifest(value), /recorded value 85/u);
});

await test("rejects a Rust baseline reduction", () => {
  const value = manifest();
  value.rustCoverageException.measuredPercent = 10;
  assert.throws(
    () => validateCoverageManifest(value),
    /recorded value 28\.06/u,
  );
});

await test("rejects malformed permanent coverage metadata", () => {
  const value = manifest();
  value.typescriptCoverage.runner = "test-inclusive";
  assert.throws(() => validateCoverageManifest(value), /runner must be/u);
});

await test("retains the permanent TypeScript gate when M3 begins", () => {
  const value = manifest();
  value.currentMilestone = "M3";
  assert.deepEqual(
    validateCoverageManifest(value).typescript.minimumPercent,
    TYPESCRIPT_TARGET,
  );
});

await test("rejects the Rust exception when M5 begins", () => {
  const value = manifest();
  value.currentMilestone = "M5";
  assert.throws(() => validateCoverageManifest(value), /expired at M5/u);
});

await test("rejects a missing coverage manifest", () => {
  assert.throws(
    () => loadCoveragePolicy("/path/that/does/not/exist.json"),
    /required coverage policy manifest is missing/u,
  );
});
