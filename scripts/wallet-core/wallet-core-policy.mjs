import { posix } from "node:path";

function isObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
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

function validatePaths(values, label, errors) {
  if (!Array.isArray(values) || values.length === 0) {
    errors.push(`${label} must be a non-empty array`);
    return;
  }
  const seen = new Set();
  for (const value of values) {
    if (!normalizedRepositoryPath(value)) {
      errors.push(`${label} contains an invalid repository path`);
    } else if (seen.has(value)) {
      errors.push(`${label} contains a duplicate path: ${value}`);
    }
    seen.add(value);
  }
}

function validateCommandKinds(values, errors) {
  if (!Array.isArray(values) || values.length === 0) {
    errors.push("commandKinds must be a non-empty array");
    return;
  }
  const seen = new Set();
  for (const value of values) {
    if (typeof value !== "string" || !/^[a-z][A-Za-z0-9]*$/u.test(value)) {
      errors.push("commandKinds must contain camelCase identifiers");
    } else if (seen.has(value)) {
      errors.push(`commandKinds contains a duplicate kind: ${value}`);
    }
    seen.add(value);
  }
}

export function validatePolicyManifest(manifest) {
  if (!isObject(manifest)) return ["boundary manifest must be an object"];

  const errors = [];
  const keys = Object.keys(manifest).sort();
  if (
    keys.length !== 4 ||
    !["commandKinds", "contracts", "schemaVersion", "sourceInputs"].every(
      (key) => keys.includes(key),
    )
  ) {
    errors.push("boundary manifest has an unexpected schema");
  }
  if (manifest.schemaVersion !== 2) {
    errors.push("schemaVersion must be 2");
  }
  if (!isObject(manifest.contracts)) {
    errors.push("contracts must be an object");
  } else {
    const contractKeys = Object.keys(manifest.contracts).sort();
    if (
      contractKeys.length !== 2 ||
      contractKeys[0] !== "rust" ||
      contractKeys[1] !== "typescript"
    ) {
      errors.push("contracts must contain exactly rust and typescript");
    }
    validatePaths(
      [manifest.contracts.rust, manifest.contracts.typescript],
      "contracts",
      errors,
    );
  }
  validateCommandKinds(manifest.commandKinds, errors);
  validatePaths(manifest.sourceInputs, "sourceInputs", errors);
  return errors;
}
