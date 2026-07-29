import { posix } from "node:path";

export const ALLOWED_COMMAND_KINDS = Object.freeze(
  "signData,createCheckPayload,parseCheckResult,createProvingPayload,canonicalizeTransaction,createSyncRequest,createShieldedSpentRequest,applyShieldedSpentResponse,setShieldedProtocolVersion,createDustSpendRequest,createDustCommitmentRequest,applyDustSpendResolution,transfer,dappTransfer,dappIntent,generateDust,balanceUnsealed,balanceSealed,submitFinalized".split(
    ",",
  ),
);
export const MILESTONES = Object.freeze(
  "M0,M1,M2,M3,M4,M5,M6,M7,M8,M9".split(","),
);
const EXPECTED_CONTRACTS = Object.freeze({
  typescript: "packages/react-native/src/commands.ts",
  rust: "crates/runtime/src/runtime/types.rs",
  extractionManifest: "scripts/m5-sanitized-target-manifest.json",
});
const EXPECTED_RUST_COVERAGE = Object.freeze({
  rule: "rust-line-coverage",
  runner: "cargo-llvm-cov",
  include: "crates/runtime/src/**/*.rs",
  ignoreFilenameRegex:
    "(^|/)(tools/bindgen|generated|fixtures|examples|tests)(/|$)|(^|/)tests?\\.rs$|\\.cargo/registry",
  minimumPercent: 80,
  baselinePercent: 81.31,
  enforcedPercent: 81.31,
  trackingIssue: "https://github.com/ADGLx/midnight-mobile/issues/40",
});
const EXPECTED_TYPESCRIPT_COVERAGE = Object.freeze({
  rule: "typescript-coverage",
  runner: "compiled-production-node-tests",
  include: "packages/react-native/.staging-build/src/**/*.js",
  setupImport: "packages/react-native/test-support/register-peer-stubs.mjs",
});
const EXPECTED_TYPESCRIPT_MINIMUM = Object.freeze({
  lines: 85,
  branches: 75,
  functions: 85,
});
const EXPECTED_TEXT_INPUTS = Object.freeze(
  `maintained|Cargo.lock|M1
maintained|Cargo.toml|M1
maintained|clippy.toml|M1
maintained|crates/runtime/Cargo.toml|M1
maintained|crates/runtime/src|M1
maintained|crates/runtime/uniffi.toml|M1
maintained|examples/expo|M3
maintained|packages/react-native/android/build.gradle|M1
maintained|packages/react-native/android/src|M3
maintained|packages/react-native/expo-module.config.json|M1
maintained|packages/react-native/ios/ExpoMidnightNative.podspec|M1
maintained|packages/react-native/ios/ExpoMidnightNativeModule.swift|M1
maintained|packages/react-native/src|M1
maintained|packages/react-native/tests|M1
maintained|rust-toolchain.toml|M4
maintained|rustfmt.toml|M1
maintained|scripts/apple-consumer.mjs|M4
maintained|scripts/apple-expo-consumer.mjs|M4
maintained|scripts/apple-native.mjs|M4
maintained|scripts/build-android-native.mjs|M4
maintained|scripts/build-apple-xcframework.mjs|M4
maintained|scripts/check-apple-xcframework.mjs|M4
maintained|scripts/check-native-distribution.mjs|M4
maintained|scripts/native-build-config.json|M4
maintained|tools/bindgen|M1
generated|packages/react-native/android/generated|M4
generated|packages/react-native/ios/generated|M4
package|packages/react-native/package.json|M1
package|packages/react-native/README.md|M1
package|packages/react-native/dist|-`
    .split("\n")
    .map((line) => {
      const [group, path, milestone] = line.split("|");
      return [group, path, milestone === "-" ? null : milestone];
    }),
);

function isObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function sameArray(actual, expected) {
  return (
    Array.isArray(actual) &&
    actual.length === expected.length &&
    actual.every((value, index) => value === expected[index])
  );
}

function normalizedRepositoryPath(value) {
  return (
    typeof value === "string" &&
    value.length > 0 &&
    !value.includes("\\") &&
    !value.includes("\0") &&
    !value.startsWith("/") &&
    !value.endsWith("/") &&
    posix.normalize(value) === value
  );
}

function checkExpected(actual, expected, label, errors) {
  for (const [name, value] of Object.entries(expected)) {
    if (actual?.[name] !== value) {
      errors.push(`${label}.${name} must be ${String(value)}`);
    }
  }
}

function validateBoundaryInputs(manifest, errors) {
  if (manifest.schemaVersion !== 1) errors.push("schemaVersion must be 1");
  if (!MILESTONES.includes(manifest.currentMilestone)) {
    errors.push("currentMilestone must be one of M0 through M9");
  }
  checkExpected(manifest.contracts, EXPECTED_CONTRACTS, "contracts", errors);
  if (!sameArray(manifest.commandKinds, ALLOWED_COMMAND_KINDS)) {
    errors.push("commandKinds must be the exact ordered 19-kind contract");
  }
  const actualInputs = Array.isArray(manifest.textInputs)
    ? manifest.textInputs.map((entry) => [
        entry?.group,
        entry?.path,
        entry?.requiredAtMilestone ?? null,
      ])
    : [];
  if (
    actualInputs.length !== EXPECTED_TEXT_INPUTS.length ||
    actualInputs.some(
      (entry, index) => !sameArray(entry, EXPECTED_TEXT_INPUTS[index]),
    )
  ) {
    errors.push(
      "textInputs must retain all maintained/generated/package roots",
    );
  }
  actualInputs
    .filter((entry) => !normalizedRepositoryPath(entry[1]))
    .forEach((entry) =>
      errors.push(`invalid text input path: ${String(entry[1])}`),
    );
}

function validateRustCoverage(manifest, current, errors) {
  const rust = manifest.rustCoverage;
  checkExpected(rust, EXPECTED_RUST_COVERAGE, "rustCoverage", errors);
  if (current < MILESTONES.indexOf("M5")) {
    errors.push("rustCoverage requires currentMilestone M5 or later");
  }
}

function validateTypescriptCoverage(manifest, errors) {
  const typescript = manifest.typescriptCoverage;
  checkExpected(
    typescript,
    EXPECTED_TYPESCRIPT_COVERAGE,
    "typescriptCoverage",
    errors,
  );
  checkExpected(
    typescript?.minimumPercent,
    EXPECTED_TYPESCRIPT_MINIMUM,
    "typescriptCoverage.minimumPercent",
    errors,
  );
}

export function validatePolicyManifest(manifest) {
  if (!isObject(manifest)) {
    return ["boundary manifest must be an object"];
  }
  const errors = [];
  validateBoundaryInputs(manifest, errors);
  const current = MILESTONES.indexOf(manifest.currentMilestone);
  validateRustCoverage(manifest, current, errors);
  validateTypescriptCoverage(manifest, errors);
  return errors;
}
