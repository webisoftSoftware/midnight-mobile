import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { isDeepStrictEqual } from "node:util";

// `scripts/release-config.json` is the single source of truth for the package
// identity, toolchain pins, and compatibility record. This module validates that
// the config is structurally complete and that every other file in the
// repository agrees with it. It deliberately does not keep a second copy of the
// expected values: duplicating them here meant a toolchain bump had to be
// applied in two places or the release gate failed for no real reason.
const REQUIRED_PACKAGE_FIELDS = Object.freeze(["name", "version", "gitTag"]);
const REQUIRED_TOOLCHAIN_FIELDS = Object.freeze([
  "node",
  "npm",
  "rust",
  "cargoLlvmCov",
  "jdk",
  "xcode",
  "cocoaPods",
  "androidCommandLineTools",
  "androidNdk",
  "androidApi",
]);
const REQUIRED_COMPATIBILITY_FIELDS = Object.freeze([
  "midnightLedger",
  "midnightLedgerRevision",
  "networks",
  "expo",
  "expoModulesCore",
  "react",
  "reactNative",
  "iosMinimum",
  "androidMinimumApi",
  "androidAbis",
]);

function isObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function exact(actual, expected, label, errors) {
  if (!isDeepStrictEqual(actual, expected)) {
    errors.push(`${label} must equal ${JSON.stringify(expected)}`);
  }
}

function requireFields(value, fields, label, errors) {
  const section = isObject(value) ? value : {};
  if (!isObject(value)) {
    errors.push(`${label} must be an object`);
    return section;
  }
  for (const field of fields) {
    const entry = section[field];
    const populated =
      (typeof entry === "string" && entry.trim().length > 0) ||
      (typeof entry === "number" && Number.isFinite(entry)) ||
      (Array.isArray(entry) && entry.length > 0);
    if (!populated) {
      errors.push(`${label}.${field} must be recorded`);
    }
  }
  return section;
}

function validateCargoLock(cargoLock, revision, errors) {
  if (typeof revision !== "string") return;
  if (typeof cargoLock !== "string" || !cargoLock.includes(revision)) {
    errors.push("Cargo lock must retain the exact Midnight Ledger revision");
  }
}

function validateCompatibilityDocument(document, compatibility, errors) {
  for (const value of Object.values(compatibility)) {
    if (typeof value === "string" && !document.includes(value)) {
      errors.push(`compatibility document is missing ${value}`);
    }
  }
}

function validateConfigShape(config, errors) {
  exact(config.schemaVersion, 1, "schemaVersion", errors);
  const pkg = requireFields(
    config.package,
    REQUIRED_PACKAGE_FIELDS,
    "package",
    errors,
  );
  const toolchains = requireFields(
    config.toolchains,
    REQUIRED_TOOLCHAIN_FIELDS,
    "toolchains",
    errors,
  );
  const compatibility = requireFields(
    config.compatibility,
    REQUIRED_COMPATIBILITY_FIELDS,
    "compatibility",
    errors,
  );
  if (typeof pkg.version === "string" && pkg.gitTag !== `v${pkg.version}`) {
    errors.push("package gitTag must be the package version prefixed with v");
  }
  exact(
    config.releaseEnvironment,
    "alpha-release",
    "releaseEnvironment",
    errors,
  );
  return { pkg, toolchains, compatibility };
}

// The distribution decision stays exact: its status is the M6 publication gate,
// not a version number that moves with a routine upgrade.
function validateDeferredDecision(config, errors) {
  const decision = requireFields(
    config.distributionDecision,
    ["status", "trackingIssue", "requiredApprovalFile"],
    "distributionDecision",
    errors,
  );
  exact(
    decision.status,
    "deferred-to-m6-destination-migration",
    "distributionDecision.status",
    errors,
  );
  exact(
    decision.requiredApprovalFile,
    "docs/DISTRIBUTION_DECISION.json",
    "distributionDecision.requiredApprovalFile",
    errors,
  );
}

// Every pinned value is checked against the config rather than a duplicated
// constant, so a toolchain bump is a one-line config edit.
function validateRepositoryAgreement(sections, inputs, errors) {
  const { pkg, toolchains, compatibility } = sections;
  exact(
    {
      name: inputs.packageMetadata?.name,
      version: inputs.packageMetadata?.version,
    },
    { name: pkg.name, version: pkg.version },
    "npm package identity",
    errors,
  );
  exact(
    inputs.rootMetadata?.packageManager,
    `npm@${toolchains.npm}`,
    "root packageManager",
    errors,
  );
  exact(
    inputs.nativeConfig?.rust?.toolchain,
    toolchains.rust,
    "native Rust toolchain",
    errors,
  );
  exact(
    inputs.nativeConfig?.android?.ndkVersion,
    toolchains.androidNdk,
    "native Android NDK",
    errors,
  );
  exact(
    inputs.nativeConfig?.apple?.deploymentTarget,
    compatibility.iosMinimum,
    "native iOS minimum",
    errors,
  );
  validateCargoLock(
    inputs.cargoLock,
    compatibility.midnightLedgerRevision,
    errors,
  );
  validateCompatibilityDocument(
    inputs.compatibilityDocument,
    compatibility,
    errors,
  );
}

export function validateReleaseConfig(config, inputs) {
  const errors = [];
  if (!isObject(config)) return ["release config must be an object"];
  const sections = validateConfigShape(config, errors);
  validateDeferredDecision(config, errors);
  validateRepositoryAgreement(sections, inputs, errors);
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
