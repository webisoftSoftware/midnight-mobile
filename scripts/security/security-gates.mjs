import { isDeepStrictEqual } from "node:util";

const EXPECTED_BRACE_VERSIONS = Object.freeze(["1.1.17", "5.0.8"]);
const EXPECTED_OVERRIDES = Object.freeze({
  "brace-expansion@<2": "1.1.17",
  "brace-expansion@>=2 <3": "2.1.3",
  "brace-expansion@>=5 <6": "5.0.8",
  uuid: "11.1.1",
});
const EXPECTED_RUSTSEC_INFORMATIONAL_ALLOWLIST = Object.freeze([
  {
    advisory: "RUSTSEC-2024-0436",
    package: "paste",
    version: "1.0.15",
    kind: "unmaintained",
    trackingIssue: "https://github.com/ADGLx/midnight-mobile/issues/47",
    removalCondition:
      "Remove when the pinned Ledger dependency graph no longer reaches paste 1.0.15 through midnight-curves, or a compatible maintained replacement is available.",
  },
  {
    advisory: "RUSTSEC-2025-0141",
    package: "bincode",
    version: "2.0.1",
    kind: "unmaintained",
    trackingIssue: "https://github.com/ADGLx/midnight-mobile/issues/47",
    removalCondition:
      "Remove when the pinned Ledger dependency graph no longer reaches bincode 2.0.1 through midnight-zk-stdlib, or a compatible maintained replacement is available.",
  },
]);
const SECRET_PATTERNS = Object.freeze([
  {
    name: "aws-access-key",
    pattern: new RegExp(["AK", "IA", "[0-9A-Z]{16}"].join(""), "gu"),
  },
  {
    name: "github-token",
    pattern: new RegExp(["gh", "[pousr]_", "[A-Za-z0-9]{20,}"].join(""), "gu"),
  },
  {
    name: "npm-token",
    pattern: new RegExp(["npm", "_", "[A-Za-z0-9]{20,}"].join(""), "gu"),
  },
  {
    name: "slack-token",
    pattern: new RegExp(
      ["xox", "[abprs]-", "[A-Za-z0-9-]{20,}"].join(""),
      "gu",
    ),
  },
  {
    name: "stripe-live-key",
    pattern: new RegExp(["sk", "_live_", "[A-Za-z0-9]{16,}"].join(""), "gu"),
  },
  {
    name: "google-api-key",
    pattern: new RegExp(["AI", "za", "[A-Za-z0-9_-]{35}"].join(""), "gu"),
  },
  {
    name: "private-key",
    pattern: new RegExp(
      ["-----BEGIN ", "(?:RSA |EC |OPENSSH )?", "PRIVATE KEY-----"].join(""),
      "gu",
    ),
  },
  {
    name: "jwt",
    pattern: new RegExp(
      ["eyJ", "[A-Za-z0-9_-]{12,}", "\\.", "[A-Za-z0-9_-]{12,}", "\\."].join(
        "",
      ),
      "gu",
    ),
  },
]);

function isObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function exact(actual, expected, label, errors) {
  if (!isDeepStrictEqual(actual, expected)) {
    errors.push(`${label} must equal ${JSON.stringify(expected)}`);
  }
}

export function validateSecurityPolicy(policy) {
  const errors = [];
  if (!isObject(policy)) return ["security policy must be an object"];
  exact(policy.schemaVersion, 1, "schemaVersion", errors);
  exact(policy.currentMilestone, "M5", "currentMilestone", errors);
  exact(
    policy.ledgerGitRevision,
    "02716c2c95d50654aeb3cb63bfd8386046e4ca7d",
    "ledgerGitRevision",
    errors,
  );
  exact(policy.cargoAuditVersion, "0.22.2", "cargoAuditVersion", errors);
  exact(
    policy.rustSecInformationalAllowlist,
    EXPECTED_RUSTSEC_INFORMATIONAL_ALLOWLIST,
    "rustSecInformationalAllowlist",
    errors,
  );
  const advisory = policy.braceExpansionAdvisory;
  exact(advisory?.advisory, "GHSA-mh99-v99m-4gvg", "advisory", errors);
  exact(advisory?.cve, "CVE-2026-14257", "cve", errors);
  exact(advisory?.source, 1_124_334, "source", errors);
  exact(
    advisory?.url,
    "https://github.com/advisories/GHSA-mh99-v99m-4gvg",
    "url",
    errors,
  );
  exact(advisory?.severity, "high", "severity", errors);
  exact(advisory?.vulnerableRange, "<=5.0.7", "vulnerableRange", errors);
  exact(
    advisory?.acceptedVersions,
    EXPECTED_BRACE_VERSIONS,
    "acceptedVersions",
    errors,
  );
  exact(
    advisory?.maximumExpandedCharacters,
    4_000_000,
    "maximumExpandedCharacters",
    errors,
  );
  exact(
    advisory?.trackingIssue,
    "https://github.com/ADGLx/midnight-mobile/issues/35",
    "trackingIssue",
    errors,
  );
  exact(
    advisory?.status,
    "upstream-advisory-metadata-blocked",
    "status",
    errors,
  );
  if (
    typeof advisory?.removalCondition !== "string" ||
    advisory.removalCondition.length < 80
  ) {
    errors.push("brace-expansion removalCondition must be concrete");
  }
  return errors;
}

export function findSecretErrors(entries) {
  const errors = [];
  for (const entry of entries) {
    for (const secret of SECRET_PATTERNS) {
      secret.pattern.lastIndex = 0;
      if (secret.pattern.test(entry.contents)) {
        errors.push(`${entry.path}: detected ${secret.name}`);
      }
    }
  }
  return errors;
}

function validateBracePackages(lockfile, sourceEvidence, policy, errors) {
  const versions = [];
  for (const [path, metadata] of Object.entries(lockfile.packages ?? {})) {
    if (!path.endsWith("/brace-expansion")) continue;
    versions.push(metadata.version);
    const evidence = sourceEvidence.get(path) ?? "";
    const normalized = evidence.replaceAll("_", "");
    if (
      !evidence.includes(policy.braceExpansionAdvisory.cve) ||
      !normalized.includes("4000000")
    ) {
      errors.push(`${path}: patched expansion-length bound is missing`);
    }
  }
  exact(
    [...new Set(versions)].sort(),
    EXPECTED_BRACE_VERSIONS,
    "brace versions",
    errors,
  );
}

function validateUuidPackages(packageMetadata, lockfile, errors) {
  exact(
    packageMetadata.devDependencies?.xcode,
    "3.0.1",
    "root xcode security pin",
    errors,
  );
  const versions = [];
  for (const [path, metadata] of Object.entries(lockfile.packages ?? {})) {
    if (!path.endsWith("/uuid")) continue;
    versions.push(metadata.version);
  }
  exact([...new Set(versions)].sort(), ["11.1.1"], "uuid versions", errors);
}

export function validateNpmDependencies(
  packageMetadata,
  lockfile,
  sourceEvidence,
  policy,
) {
  const errors = [];
  exact(packageMetadata.overrides, EXPECTED_OVERRIDES, "npm overrides", errors);
  for (const [path, metadata] of Object.entries(lockfile.packages ?? {})) {
    if (!path.startsWith("node_modules/") || metadata.version === undefined) {
      continue;
    }
    if (
      typeof metadata.resolved !== "string" ||
      !metadata.resolved.startsWith("https://registry.npmjs.org/")
    ) {
      errors.push(`${path}: dependency source is not the npm registry`);
    }
    if (
      typeof metadata.integrity !== "string" ||
      !metadata.integrity.startsWith("sha512-")
    ) {
      errors.push(`${path}: dependency integrity is missing`);
    }
    if (typeof metadata.license !== "string" || metadata.license.length === 0) {
      errors.push(`${path}: dependency license is missing`);
    }
  }
  validateBracePackages(lockfile, sourceEvidence, policy, errors);
  validateUuidPackages(packageMetadata, lockfile, errors);
  return errors;
}

export function validateCargoDependencies(metadata, policy) {
  const errors = [];
  if (!Array.isArray(metadata?.packages) || metadata.packages.length === 0) {
    return ["Cargo metadata contains no packages"];
  }
  for (const dependency of metadata.packages) {
    if (
      typeof dependency.license !== "string" &&
      dependency.license_file === null
    ) {
      errors.push(
        `${String(dependency.name)}@${String(dependency.version)}: license is missing`,
      );
    }
    if (
      typeof dependency.source === "string" &&
      dependency.source.startsWith("git+") &&
      !dependency.source.includes(policy.ledgerGitRevision)
    ) {
      errors.push(
        `${String(dependency.name)}@${String(dependency.version)}: unapproved Git source`,
      );
    }
  }
  return errors;
}
