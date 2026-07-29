import assert from "node:assert/strict";
import test from "node:test";

import {
  RUST_BASELINE,
  RUST_IGNORE_FILENAME_REGEX,
  RUST_TARGET,
  TYPESCRIPT_TARGET,
  loadCoveragePolicy,
  validateCoverageManifest,
} from "./coverage-policy.mjs";

function manifest() {
  return {
    currentMilestone: "M5",
    rustCoverage: {
      rule: "rust-line-coverage",
      runner: "cargo-llvm-cov",
      include: "crates/runtime/src/**/*.rs",
      ignoreFilenameRegex: RUST_IGNORE_FILENAME_REGEX,
      minimumPercent: RUST_TARGET,
      baselinePercent: RUST_BASELINE,
      enforcedPercent: RUST_BASELINE,
      trackingIssue: "https://github.com/ADGLx/midnight-mobile/issues/40",
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

await test("accepts permanent TypeScript and Rust coverage gates", () => {
  const policy = validateCoverageManifest(manifest());
  assert.equal(policy.currentMilestone, "M5");
  assert.deepEqual(policy.typescript.minimumPercent, TYPESCRIPT_TARGET);
  assert.equal(policy.rust.minimumPercent, RUST_TARGET);
  assert.equal(policy.rust.baselinePercent, RUST_BASELINE);
  assert.equal(policy.rust.enforcedPercent, RUST_BASELINE);
  assert.equal(policy.rust.ignoreFilenameRegex, RUST_IGNORE_FILENAME_REGEX);
});

await test("rejects a TypeScript threshold reduction", () => {
  const value = manifest();
  value.typescriptCoverage.minimumPercent.lines = 70;
  assert.throws(() => validateCoverageManifest(value), /recorded value 85/u);
});

await test("rejects a Rust baseline reduction", () => {
  const value = manifest();
  value.rustCoverage.baselinePercent = RUST_TARGET;
  assert.throws(
    () => validateCoverageManifest(value),
    /recorded value 81\.31/u,
  );
});

await test("rejects a Rust enforcement reduction", () => {
  const value = manifest();
  value.rustCoverage.enforcedPercent = RUST_TARGET;
  assert.throws(
    () => validateCoverageManifest(value),
    /recorded value 81\.31/u,
  );
});

await test("rejects drift in the Rust exclusion scope", () => {
  const value = manifest();
  value.rustCoverage.ignoreFilenameRegex = ".*";
  assert.throws(
    () => validateCoverageManifest(value),
    /ignoreFilenameRegex must be/u,
  );
});

await test("rejects malformed permanent coverage metadata", () => {
  const value = manifest();
  value.typescriptCoverage.runner = "test-inclusive";
  assert.throws(() => validateCoverageManifest(value), /runner must be/u);
});

await test("retains both permanent gates after M5", () => {
  const value = manifest();
  value.currentMilestone = "M6";
  const policy = validateCoverageManifest(value);
  assert.deepEqual(policy.typescript.minimumPercent, TYPESCRIPT_TARGET);
  assert.equal(policy.rust.enforcedPercent, RUST_BASELINE);
});

await test("rejects the permanent Rust gate before M5", () => {
  const value = manifest();
  value.currentMilestone = "M4";
  assert.throws(
    () => validateCoverageManifest(value),
    /currentMilestone M5 or later/u,
  );
});

await test("rejects a missing coverage manifest", () => {
  assert.throws(
    () => loadCoveragePolicy("/path/that/does/not/exist.json"),
    /required coverage policy manifest is missing/u,
  );
});
