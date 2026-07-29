import { posix } from "node:path";

export const ALLOWED_COMMAND_KINDS = Object.freeze(
  "signData,createCheckPayload,parseCheckResult,createProvingPayload,canonicalizeTransaction,createSyncRequest,createShieldedSpentRequest,applyShieldedSpentResponse,setShieldedProtocolVersion,createDustSpendRequest,createDustCommitmentRequest,applyDustSpendResolution,transfer,dappTransfer,dappIntent,generateDust,balanceUnsealed,balanceSealed,submitFinalized".split(
    ",",
  ),
);
export const MILESTONES = Object.freeze(
  "M0,M1,M2,M3,M4,M5,M6,M7,M8".split(","),
);
const EXPECTED_CONTRACTS = Object.freeze({
  typescript: "packages/react-native/src/commands.ts",
  rust: "crates/runtime/src/runtime/types.rs",
  extractionManifest: "scripts/m1-sanitized-target-manifest.json",
});
const EXPECTED_RUST_EXCEPTION = Object.freeze({
  rule: "rust-line-coverage",
  measuredPercent: 14.73,
  minimumPercent: 80,
  trackingIssue: "https://github.com/ADGLx/midnight-mobile/issues/7",
  activeFromMilestone: "M1",
  expiresAtMilestone: "M5",
});
const EXPECTED_TYPESCRIPT_EXCEPTION = Object.freeze({
  rule: "typescript-coverage",
  trackingIssue: "https://github.com/ADGLx/midnight-mobile/issues/16",
  runner: "compiled-node-tests",
  activeFromMilestone: "M1",
  expiresAtMilestone: "M3",
});
const EXPECTED_TYPESCRIPT_MEASURED = Object.freeze({
  lines: 71.3,
  branches: 73.23,
  functions: 63.19,
});
const EXPECTED_TYPESCRIPT_REQUIRED = Object.freeze({
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
maintained|packages/react-native/android/build.gradle|M1
maintained|packages/react-native/android/src|M3
maintained|packages/react-native/expo-module.config.json|M1
maintained|packages/react-native/ios/ExpoMidnightNative.podspec|M1
maintained|packages/react-native/ios/ExpoMidnightNativeModule.swift|M1
maintained|packages/react-native/src|M1
maintained|packages/react-native/tests|M1
maintained|rustfmt.toml|M1
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

function checkRemovalCondition(exception, label, errors) {
  if (
    typeof exception?.removalCondition !== "string" ||
    exception.removalCondition.trim().length < 40
  ) {
    errors.push(`${label}.removalCondition must be concrete`);
  }
}

function validateBoundaryInputs(manifest, errors) {
  if (manifest.schemaVersion !== 1) errors.push("schemaVersion must be 1");
  if (!MILESTONES.includes(manifest.currentMilestone)) {
    errors.push("currentMilestone must be one of M0 through M8");
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

function validateRustException(manifest, current, errors) {
  const rust = manifest.rustCoverageException;
  checkExpected(rust, EXPECTED_RUST_EXCEPTION, "rustCoverageException", errors);
  checkRemovalCondition(rust, "rustCoverageException", errors);
  const rustExpiry = MILESTONES.indexOf(rust?.expiresAtMilestone);
  if (current >= rustExpiry && rustExpiry >= 0) {
    errors.push(
      `Rust coverage exception expired at ${rust.expiresAtMilestone}`,
    );
  }
}

function validateTypescriptException(manifest, current, errors) {
  const typescript = manifest.typescriptCoverageException;
  checkExpected(
    typescript,
    EXPECTED_TYPESCRIPT_EXCEPTION,
    "typescriptCoverageException",
    errors,
  );
  checkExpected(
    typescript?.measuredPercent,
    EXPECTED_TYPESCRIPT_MEASURED,
    "typescriptCoverageException.measuredPercent",
    errors,
  );
  checkExpected(
    typescript?.requiredPercent,
    EXPECTED_TYPESCRIPT_REQUIRED,
    "typescriptCoverageException.requiredPercent",
    errors,
  );
  checkRemovalCondition(typescript, "typescriptCoverageException", errors);
  const typescriptExpiry = MILESTONES.indexOf(typescript?.expiresAtMilestone);
  if (current >= typescriptExpiry && typescriptExpiry >= 0) {
    errors.push(
      `TypeScript coverage exception expired at ${typescript.expiresAtMilestone}`,
    );
  }
}

export function validatePolicyManifest(manifest) {
  if (!isObject(manifest)) {
    return ["boundary manifest must be an object"];
  }
  const errors = [];
  validateBoundaryInputs(manifest, errors);
  const current = MILESTONES.indexOf(manifest.currentMilestone);
  validateRustException(manifest, current, errors);
  validateTypescriptException(manifest, current, errors);
  return errors;
}
