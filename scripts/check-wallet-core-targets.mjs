import { lstatSync, readdirSync, realpathSync } from "node:fs";
import { posix, relative, resolve } from "node:path";

const EXPECTED_AUTHORITY = Object.freeze({
  sourceRepository: "https://github.com/webisoftSoftware/one-am-wallet",
  assessedCommit: "dc9c0dd34dba9e19304859106ca2531f48590599",
  componentRoot: "apps/mobile/modules/expo-midnight-native",
  sourceArchiveSha256:
    "57f2cfd30d1d920ad3041a6db52bccbede90613930148e2b6e41fc5bb683fb74",
});
const BASE_ROOTS = Object.freeze([
  "crates/runtime",
  "packages/react-native",
  "tools/bindgen",
]);
const M3_ROOTS = Object.freeze([...BASE_ROOTS, "examples/expo"]);
const EXPECTED_ROOT_SETS = Object.freeze([BASE_ROOTS, M3_ROOTS]);
const EXPECTED_FILES = Object.freeze([
  "Cargo.lock",
  "Cargo.toml",
  "clippy.toml",
  "rustfmt.toml",
]);
const EXPECTED_ORIGINS = Object.freeze([
  "assessed-sanitized",
  "generated-sanitized",
  "repository-authored",
]);
const OUTPUT_DIRECTORIES = new Set([
  "_input",
  ".declaration-build",
  ".expo",
  ".platform-build",
  ".staging-build",
  ".test-build",
  "build",
  "dist",
  "staging",
  "staging-build",
  "target",
]);

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

function normalizedTarget(path) {
  return (
    typeof path === "string" &&
    path.length > 0 &&
    !path.includes("\\") &&
    !path.includes("\0") &&
    !path.startsWith("/") &&
    !path.endsWith("/") &&
    posix.normalize(path) === path
  );
}

function forbiddenOutputSegment(path) {
  const directories = path.replaceAll("\\", "/").split("/").slice(0, -1);
  return directories.find((segment) => OUTPUT_DIRECTORIES.has(segment));
}

function exactObjectKeys(value, expected) {
  return (
    isObject(value) &&
    sameArray(Object.keys(value).sort(), [...expected].sort())
  );
}

function validateManifestHeader(manifest, errors) {
  const topKeys = [
    "authority",
    "entries",
    "entryCount",
    "originClasses",
    "schemaVersion",
    "targetFiles",
    "targetRoots",
  ];
  if (!exactObjectKeys(manifest, topKeys)) {
    errors.push("sanitized target manifest has an unexpected schema");
  }
  if (manifest.schemaVersion !== 1) {
    errors.push("sanitized target schemaVersion must be 1");
  }
  if (!exactObjectKeys(manifest.authority, Object.keys(EXPECTED_AUTHORITY))) {
    errors.push("sanitized target authority has an unexpected schema");
  }
  for (const [name, expected] of Object.entries(EXPECTED_AUTHORITY)) {
    if (manifest.authority?.[name] !== expected) {
      errors.push(`sanitized target authority.${name} must be ${expected}`);
    }
  }
  if (
    !EXPECTED_ROOT_SETS.some((roots) => sameArray(manifest.targetRoots, roots))
  ) {
    errors.push(
      "sanitized targetRoots must be the exact approved legacy or M3 roots",
    );
  }
  if (!sameArray(manifest.targetFiles, EXPECTED_FILES)) {
    errors.push("sanitized targetFiles must be the exact workspace files");
  }
  if (!sameArray(manifest.originClasses, EXPECTED_ORIGINS)) {
    errors.push("sanitized originClasses must be the exact approved classes");
  }
  const entries = Array.isArray(manifest.entries) ? manifest.entries : [];
  if (manifest.entryCount !== entries.length || entries.length === 0) {
    errors.push("sanitized entryCount must match a non-empty entries array");
  }
  return entries;
}

function isApprovedTarget(path, targetRoots) {
  return (
    EXPECTED_FILES.includes(path) ||
    targetRoots.some((root) => path === root || path.startsWith(`${root}/`))
  );
}

function validateEntry(entry, index, targetRoots, targets, seen, errors) {
  const label = `sanitized entries[${index}]`;
  if (!exactObjectKeys(entry, ["originClass", "targetPath"])) {
    errors.push(`${label} has an unexpected schema`);
    return;
  }
  const { originClass, targetPath } = entry;
  if (
    !normalizedTarget(targetPath) ||
    !isApprovedTarget(targetPath, targetRoots)
  ) {
    errors.push(`${label}.targetPath is not an approved target`);
  }
  const segment = forbiddenOutputSegment(targetPath);
  if (segment !== undefined) {
    errors.push(`${label}.targetPath contains output directory ${segment}`);
  }
  if (!EXPECTED_ORIGINS.includes(originClass)) {
    errors.push(`${label}.originClass is not approved`);
  }
  if (seen.has(targetPath)) {
    errors.push(`duplicate sanitized target: ${String(targetPath)}`);
  }
  if (index > 0 && targets[index - 1] > targetPath) {
    errors.push("sanitized target entries must be sorted");
  }
  seen.add(targetPath);
  targets.push(targetPath);
}

function compareTargetSets(targets, seen, actualTargets, errors) {
  const actual = new Set(actualTargets);
  targets
    .filter((path) => !actual.has(path))
    .forEach((path) => errors.push(`sanitized target is missing: ${path}`));
  [...actual]
    .filter((path) => !seen.has(path))
    .forEach((path) => errors.push(`untracked sanitized target: ${path}`));
}

export function validateSanitizedTargetManifest(
  manifest,
  actualTargets = undefined,
) {
  if (!isObject(manifest)) {
    return ["sanitized target manifest must be an object"];
  }
  const errors = [];
  const entries = validateManifestHeader(manifest, errors);
  const targetRoots = Array.isArray(manifest.targetRoots)
    ? manifest.targetRoots
    : [];
  const targets = [];
  const seen = new Set();
  entries.forEach((entry, index) =>
    validateEntry(entry, index, targetRoots, targets, seen, errors),
  );
  if (actualTargets !== undefined) {
    compareTargetSets(targets, seen, actualTargets, errors);
  }
  return errors;
}

function walk(path, repositoryRoot, targets, visited) {
  const stat = lstatSync(path);
  if (stat.isSymbolicLink()) {
    const real = realpathSync(path);
    if (visited.has(real)) return;
    visited.add(real);
    walk(real, repositoryRoot, targets, visited);
  } else if (stat.isDirectory()) {
    for (const entry of readdirSync(path, { withFileTypes: true })) {
      if (entry.isDirectory() && OUTPUT_DIRECTORIES.has(entry.name)) continue;
      walk(resolve(path, entry.name), repositoryRoot, targets, visited);
    }
  } else if (stat.isFile()) {
    targets.push(relative(repositoryRoot, path).replaceAll("\\", "/"));
  }
}

export function listSanitizedTargets(repositoryRoot, targetRoots) {
  const targets = [];
  for (const root of targetRoots) {
    walk(resolve(repositoryRoot, root), repositoryRoot, targets, new Set());
  }
  return targets.sort();
}
