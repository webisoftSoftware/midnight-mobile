import { existsSync, readFileSync } from "node:fs";

export const COVERAGE_MANIFEST_PATH =
  "scripts/wallet-core-boundary-manifest.json";
export const TYPESCRIPT_M1_BASELINE = Object.freeze({
  lines: 71.3,
  branches: 73.23,
  functions: 63.19,
});
export const TYPESCRIPT_TARGET = Object.freeze({
  lines: 85,
  branches: 75,
  functions: 85,
});
export const RUST_M1_BASELINE = 14.73;
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

function validateTypeScriptException(value, currentMilestone) {
  const field = "typescriptCoverageException";
  const exception = requireObject(value, field);
  requireExactString(exception.rule, "typescript-coverage", `${field}.rule`);
  requireExactString(
    exception.trackingIssue,
    "https://github.com/ADGLx/midnight-mobile/issues/16",
    `${field}.trackingIssue`,
  );
  requireExactString(
    exception.runner,
    "compiled-node-tests",
    `${field}.runner`,
  );
  requireExactString(
    exception.activeFromMilestone,
    "M1",
    `${field}.activeFromMilestone`,
  );
  requireExactString(
    exception.expiresAtMilestone,
    "M3",
    `${field}.expiresAtMilestone`,
  );
  requireConcreteText(exception.removalCondition, `${field}.removalCondition`);
  validateActiveWindow(exception, currentMilestone, field);
  const measured = requireObject(
    exception.measuredPercent,
    `${field}.measuredPercent`,
  );
  const required = requireObject(
    exception.requiredPercent,
    `${field}.requiredPercent`,
  );
  return {
    runner: exception.runner,
    measuredPercent: {
      lines: requirePercentage(
        measured.lines,
        TYPESCRIPT_M1_BASELINE.lines,
        `${field}.measuredPercent.lines`,
      ),
      branches: requirePercentage(
        measured.branches,
        TYPESCRIPT_M1_BASELINE.branches,
        `${field}.measuredPercent.branches`,
      ),
      functions: requirePercentage(
        measured.functions,
        TYPESCRIPT_M1_BASELINE.functions,
        `${field}.measuredPercent.functions`,
      ),
    },
    requiredPercent: {
      lines: requirePercentage(
        required.lines,
        TYPESCRIPT_TARGET.lines,
        `${field}.requiredPercent.lines`,
      ),
      branches: requirePercentage(
        required.branches,
        TYPESCRIPT_TARGET.branches,
        `${field}.requiredPercent.branches`,
      ),
      functions: requirePercentage(
        required.functions,
        TYPESCRIPT_TARGET.functions,
        `${field}.requiredPercent.functions`,
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
    "https://github.com/ADGLx/midnight-mobile/issues/7",
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
      RUST_M1_BASELINE,
      `${field}.measuredPercent`,
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
  const typescript = validateTypeScriptException(
    manifest.typescriptCoverageException,
    currentMilestone,
  );
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
