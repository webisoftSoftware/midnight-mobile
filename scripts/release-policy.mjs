import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { isDeepStrictEqual } from "node:util";

const EXPECTED_PACKAGE = Object.freeze({
  name: "@1am/midnight-mobile",
  version: "0.1.0-alpha.1",
  gitTag: "v0.1.0-alpha.1",
});
const EXPECTED_TOOLCHAINS = Object.freeze({
  node: "22",
  npm: "11.12.1",
  rust: "1.97.1",
  cargoLlvmCov: "0.8.6",
  jdk: "17.0.19+7",
  xcode: "16.4",
  cocoaPods: "1.16.2",
  androidNdk: "27.1.12297006",
  androidApi: 24,
});
const EXPECTED_COMPATIBILITY = Object.freeze({
  midnightLedger: "8.1.0",
  midnightLedgerRevision: "02716c2c95d50654aeb3cb63bfd8386046e4ca7d",
  networks: ["preview", "preprod", "mainnet"],
  expo: "55.0.28",
  expoModulesCore: "55.0.25",
  react: "19.2.0",
  reactNative: "0.83.6",
  iosMinimum: "15.1",
  androidMinimumApi: 24,
  androidAbis: ["arm64-v8a", "x86_64"],
});

function isObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function exact(actual, expected, label, errors) {
  if (!isDeepStrictEqual(actual, expected)) {
    errors.push(`${label} must equal ${JSON.stringify(expected)}`);
  }
}

function validateCargoLock(cargoLock, errors) {
  if (
    typeof cargoLock !== "string" ||
    !cargoLock.includes(EXPECTED_COMPATIBILITY.midnightLedgerRevision)
  ) {
    errors.push("Cargo lock must retain the exact Midnight Ledger revision");
  }
}

function validateCompatibilityDocument(document, errors) {
  for (const value of Object.values(EXPECTED_COMPATIBILITY)) {
    if (typeof value === "string" && !document.includes(value)) {
      errors.push(`compatibility document is missing ${value}`);
    }
  }
}

export function validateReleaseConfig(config, inputs) {
  const errors = [];
  if (!isObject(config)) return ["release config must be an object"];
  exact(config.schemaVersion, 1, "schemaVersion", errors);
  exact(config.package, EXPECTED_PACKAGE, "package", errors);
  exact(config.toolchains, EXPECTED_TOOLCHAINS, "toolchains", errors);
  exact(config.compatibility, EXPECTED_COMPATIBILITY, "compatibility", errors);
  exact(
    config.releaseEnvironment,
    "alpha-release",
    "releaseEnvironment",
    errors,
  );
  exact(
    config.distributionDecision,
    {
      status: "deferred-to-m6-destination-migration",
      trackingIssue: "https://github.com/ADGLx/midnight-mobile/issues/38",
      requiredApprovalFile: "docs/DISTRIBUTION_DECISION.json",
    },
    "distributionDecision",
    errors,
  );
  exact(
    {
      name: inputs.packageMetadata?.name,
      version: inputs.packageMetadata?.version,
    },
    {
      name: EXPECTED_PACKAGE.name,
      version: EXPECTED_PACKAGE.version,
    },
    "npm package identity",
    errors,
  );
  exact(
    inputs.rootMetadata?.packageManager,
    `npm@${EXPECTED_TOOLCHAINS.npm}`,
    "root packageManager",
    errors,
  );
  exact(
    inputs.nativeConfig?.rust?.toolchain,
    EXPECTED_TOOLCHAINS.rust,
    "native Rust toolchain",
    errors,
  );
  exact(
    inputs.nativeConfig?.android?.ndkVersion,
    EXPECTED_TOOLCHAINS.androidNdk,
    "native Android NDK",
    errors,
  );
  exact(
    inputs.nativeConfig?.apple?.deploymentTarget,
    EXPECTED_COMPATIBILITY.iosMinimum,
    "native iOS minimum",
    errors,
  );
  validateCargoLock(inputs.cargoLock, errors);
  validateCompatibilityDocument(inputs.compatibilityDocument, errors);
  return errors;
}

export function validateReleaseTag(tag, tagsAtHead, config) {
  if (tag === undefined) return [];
  const errors = [];
  exact(tag, config.package.gitTag, "release tag", errors);
  if (!tagsAtHead.includes(tag)) {
    errors.push("release tag must point at the checked-out commit");
  }
  return errors;
}

export function validateDistributionDecision(value, config) {
  const errors = [];
  if (!isObject(value)) return ["distribution decision must be an object"];
  exact(value.schemaVersion, 1, "distribution schemaVersion", errors);
  exact(value.status, "approved", "distribution status", errors);
  exact(
    value.packageVersion,
    config.package.version,
    "distribution packageVersion",
    errors,
  );
  for (const field of [
    "copyrightOwner",
    "sourceLicenseExpression",
    "packageLicenseExpression",
    "reviewedBy",
    "reviewedAt",
  ]) {
    if (typeof value[field] !== "string" || value[field].trim().length < 3) {
      errors.push(`distribution ${field} must be recorded`);
    }
  }
  return errors;
}

function componentName(path, metadata) {
  if (typeof metadata.name === "string") return metadata.name;
  return path.split("node_modules/").at(-1) ?? path;
}

export function npmComponents(lockfile) {
  return Object.entries(lockfile.packages ?? {})
    .filter(([, metadata]) => typeof metadata.version === "string")
    .map(([path, metadata]) => ({
      ecosystem: "npm",
      name: componentName(path, metadata),
      version: metadata.version,
      license: metadata.license ?? "NOASSERTION",
      source: metadata.resolved ?? "workspace",
      integrity: metadata.integrity ?? null,
      path: path.length === 0 ? "." : path,
    }))
    .sort((left, right) =>
      `${left.name}\0${left.version}\0${left.path}`.localeCompare(
        `${right.name}\0${right.version}\0${right.path}`,
      ),
    );
}

export function cargoComponents(metadata) {
  return metadata.packages
    .map((dependency) => ({
      ecosystem: "cargo",
      name: dependency.name,
      version: dependency.version,
      license: dependency.license ?? "NOASSERTION",
      source: dependency.source ?? "workspace",
      integrity: null,
      path: null,
    }))
    .sort((left, right) =>
      `${left.name}\0${left.version}\0${left.source}`.localeCompare(
        `${right.name}\0${right.version}\0${right.source}`,
      ),
    );
}

export function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

export function sourceTreeFingerprint(root, paths) {
  const hash = createHash("sha256");
  for (const path of [...paths].sort()) {
    hash.update(path);
    hash.update("\0");
    hash.update(readFileSync(`${root}/${path}`));
    hash.update("\0");
  }
  return hash.digest("hex");
}
