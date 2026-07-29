import { existsSync, readFileSync } from "node:fs";

export const COVERAGE_MANIFEST_PATH =
  "scripts/wallet-core-boundary-manifest.json";
export const TYPESCRIPT_TARGET = Object.freeze({
  lines: 85,
  branches: 75,
  functions: 85,
});
export const RUST_M2_MEASURED = 28.06;
export const RUST_M2_ENFORCED = 28.05;
export const RUST_TARGET = 80;

const MILESTONES = Object.freeze([
  "M0",
  "M1",
  "M2",
  "M3",
  "M4",
  "M5",
  "M6",
  "M7",
  "M8",
]);

function policyError(message) {
  throw new Error(`${COVERAGE_MANIFEST_PATH}: ${message}`);
}

function requireObject(value, field) {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    policyError(`${field} must be an object`);
  }
  return value;
}

function requireExactString(value, expected, field) {
  if (value !== expected) {
    policyError(`${field} must be ${JSON.stringify(expected)}`);
  }
}

function requireConcreteText(value, field) {
  if (typeof value !== "string" || value.trim().length < 20) {
    policyError(`${field} must be concrete and at least 20 characters`);
  }
}

function requirePercentage(value, expected, field) {
  if (
    typeof value !== "number" ||
    !Number.isFinite(value) ||
    value < 0 ||
    value > 100
  ) {
    policyError(`${field} must be a finite percentage from 0 through 100`);
  }
  if (value !== expected) {
    policyError(
      `${field} must preserve the recorded value ${String(expected)}`,
    );
  }
  return value;
}

function milestoneIndex(value, field) {
  if (typeof value !== "string" || !MILESTONES.includes(value)) {
    policyError(`${field} must be one of ${MILESTONES.join(", ")}`);
  }
  return MILESTONES.indexOf(value);
}

function validateActiveWindow(exception, currentMilestone, field) {
  const current = milestoneIndex(currentMilestone, "currentMilestone");
  const active = milestoneIndex(
    exception.activeFromMilestone,
    `${field}.activeFromMilestone`,
  );
  const expiry = milestoneIndex(
    exception.expiresAtMilestone,
    `${field}.expiresAtMilestone`,
  );
  if (active >= expiry) {
    policyError(`${field} must expire after it becomes active`);
  }
  if (current < active) {
    policyError(`${field} is not active at ${currentMilestone}`);
  }
  if (current >= expiry) {
    policyError(`${field} expired at ${exception.expiresAtMilestone}`);
  }
}

function validateTypeScriptCoverage(value) {
  const field = "typescriptCoverage";
  const coverage = requireObject(value, field);
  requireExactString(coverage.rule, "typescript-coverage", `${field}.rule`);
  requireExactString(
    coverage.runner,
    "compiled-production-node-tests",
    `${field}.runner`,
  );
  requireExactString(
    coverage.include,
    "packages/react-native/.staging-build/src/**/*.js",
    `${field}.include`,
  );
  requireExactString(
    coverage.setupImport,
    "packages/react-native/test-support/register-peer-stubs.mjs",
    `${field}.setupImport`,
  );
  const minimum = requireObject(
    coverage.minimumPercent,
    `${field}.minimumPercent`,
  );
  return {
    runner: coverage.runner,
    include: coverage.include,
    setupImport: coverage.setupImport,
    minimumPercent: {
      lines: requirePercentage(
        minimum.lines,
        TYPESCRIPT_TARGET.lines,
        `${field}.minimumPercent.lines`,
      ),
      branches: requirePercentage(
        minimum.branches,
        TYPESCRIPT_TARGET.branches,
        `${field}.minimumPercent.branches`,
      ),
      functions: requirePercentage(
        minimum.functions,
        TYPESCRIPT_TARGET.functions,
        `${field}.minimumPercent.functions`,
      ),
    },
  };
}

function validateRustException(value, currentMilestone) {
  const field = "rustCoverageException";
  const exception = requireObject(value, field);
  requireExactString(exception.rule, "rust-line-coverage", `${field}.rule`);
  requireExactString(
    exception.trackingIssue,
    "https://github.com/ADGLx/midnight-mobile/issues/40",
    `${field}.trackingIssue`,
  );
  requireExactString(
    exception.activeFromMilestone,
    "M1",
    `${field}.activeFromMilestone`,
  );
  requireExactString(
    exception.expiresAtMilestone,
    "M5",
    `${field}.expiresAtMilestone`,
  );
  requireConcreteText(exception.removalCondition, `${field}.removalCondition`);
  validateActiveWindow(exception, currentMilestone, field);
  return {
    measuredPercent: requirePercentage(
      exception.measuredPercent,
      RUST_M2_MEASURED,
      `${field}.measuredPercent`,
    ),
    enforcedPercent: requirePercentage(
      exception.enforcedPercent,
      RUST_M2_ENFORCED,
      `${field}.enforcedPercent`,
    ),
    requiredPercent: requirePercentage(
      exception.minimumPercent,
      RUST_TARGET,
      `${field}.minimumPercent`,
    ),
  };
}

export function validateCoverageManifest(value) {
  const manifest = requireObject(value, "manifest");
  const currentMilestone = manifest.currentMilestone;
  milestoneIndex(currentMilestone, "currentMilestone");
  const rust = validateRustException(
    manifest.rustCoverageException,
    currentMilestone,
  );
  const typescript = validateTypeScriptCoverage(manifest.typescriptCoverage);
  return {
    currentMilestone,
    typescript,
    rust,
  };
}

export function loadCoveragePolicy(path = COVERAGE_MANIFEST_PATH) {
  if (!existsSync(path)) {
    policyError("required coverage policy manifest is missing");
  }
  let value;
  try {
    value = JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    const detail =
      error instanceof Error ? error.message : "unknown JSON parse failure";
    policyError(`invalid JSON: ${detail}`);
  }
  return validateCoverageManifest(value);
}
