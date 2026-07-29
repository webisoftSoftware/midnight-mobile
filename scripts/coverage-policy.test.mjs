import assert from "node:assert/strict";
import test from "node:test";

import {
  RUST_M1_BASELINE,
  RUST_TARGET,
  TYPESCRIPT_M1_BASELINE,
  TYPESCRIPT_TARGET,
  loadCoveragePolicy,
  validateCoverageManifest,
} from "./coverage-policy.mjs";

function manifest() {
  return {
    currentMilestone: "M1",
    rustCoverageException: {
      rule: "rust-line-coverage",
      minimumPercent: RUST_TARGET,
      measuredPercent: RUST_M1_BASELINE,
      trackingIssue: "https://github.com/ADGLx/midnight-mobile/issues/7",
      activeFromMilestone: "M1",
      expiresAtMilestone: "M5",
      removalCondition:
        "Raise measured Rust runtime line coverage to the required target before M5 begins.",
    },
    typescriptCoverageException: {
      rule: "typescript-coverage",
      trackingIssue: "https://github.com/ADGLx/midnight-mobile/issues/16",
      runner: "compiled-node-tests",
      activeFromMilestone: "M1",
      expiresAtMilestone: "M3",
      measuredPercent: { ...TYPESCRIPT_M1_BASELINE },
      requiredPercent: { ...TYPESCRIPT_TARGET },
      removalCondition:
        "Replace the temporary compiled Node runner after package test infrastructure reaches its target.",
    },
  };
}

await test("accepts the reviewed active M1 coverage exceptions", () => {
  const policy = validateCoverageManifest(manifest());
  assert.equal(policy.currentMilestone, "M1");
  assert.deepEqual(policy.typescript.measuredPercent, TYPESCRIPT_M1_BASELINE);
  assert.equal(policy.rust.measuredPercent, RUST_M1_BASELINE);
});

await test("rejects a TypeScript baseline reduction", () => {
  const value = manifest();
  value.typescriptCoverageException.measuredPercent.lines = 70;
  assert.throws(() => validateCoverageManifest(value), /recorded value 71\.3/u);
});

await test("rejects a Rust baseline reduction", () => {
  const value = manifest();
  value.rustCoverageException.measuredPercent = 10;
  assert.throws(
    () => validateCoverageManifest(value),
    /recorded value 14\.73/u,
  );
});

await test("rejects malformed exception metadata", () => {
  const value = manifest();
  value.typescriptCoverageException.trackingIssue = "#16";
  assert.throws(
    () => validateCoverageManifest(value),
    /trackingIssue must be/u,
  );
});

await test("rejects the TypeScript exception when M3 begins", () => {
  const value = manifest();
  value.currentMilestone = "M3";
  assert.throws(() => validateCoverageManifest(value), /expired at M3/u);
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
