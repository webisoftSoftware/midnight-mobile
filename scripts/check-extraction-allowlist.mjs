import { readFileSync } from "node:fs";
import { posix, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const MANIFEST_PATH = "scripts/extraction-allowlist-manifest.json";
const AUDITED_PATHS_FIXTURE =
  "scripts/fixtures/extraction-audited-component-paths.txt";
const ALLOWLIST_DOCUMENT = "docs/EXTRACTION_ALLOWLIST.md";

const EXPECTED_AUTHORITY = Object.freeze({
  sourceRepository: "https://github.com/webisoftSoftware/one-am-wallet",
  assessedCommit: "dc9c0dd34dba9e19304859106ca2531f48590599",
  componentRoot: "apps/mobile/modules/expo-midnight-native",
});
const EXPECTED_COUNTS = Object.freeze({
  candidates: 69,
  exactExclusions: 39,
  componentLevelExclusions: 2,
  total: 110,
});
const REQUIRED_UNIFFI_PATH =
  "apps/mobile/modules/expo-midnight-native/rust/runtime/uniffi.toml";
const INCORRECT_UNIFFI_PATH =
  "apps/mobile/modules/expo-midnight-native/rust/uniffi.toml";
const EXPECTED_COMPONENT_EXCLUSIONS = Object.freeze([
  "apps/mobile/modules/expo-midnight-native/.gitignore",
  "apps/mobile/modules/expo-midnight-native/README.md",
]);
const PARTITION_NAMES = Object.freeze([
  "candidates",
  "exactExclusions",
  "componentLevelExclusions",
]);
const globPattern = /[*?[\]{}!]/u;

function addError(errors, message) {
  errors.push(message);
}

function isObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function readStringArray(manifest, name, errors) {
  const value = isObject(manifest) ? manifest[name] : undefined;
  if (!Array.isArray(value)) {
    addError(errors, `${name} must be an array`);
    return [];
  }
  return value;
}

function validateAuthority(manifest, errors) {
  if (!isObject(manifest)) {
    addError(errors, "manifest must be an object");
    return;
  }
  if (manifest.schemaVersion !== 1) {
    addError(errors, "schemaVersion must be 1");
  }
  if (!isObject(manifest.authority)) {
    addError(errors, "authority must be an object");
    return;
  }
  for (const [field, expected] of Object.entries(EXPECTED_AUTHORITY)) {
    if (manifest.authority[field] !== expected) {
      addError(
        errors,
        `authority.${field} must be ${JSON.stringify(expected)}`,
      );
    }
  }
}

function validatePath(path, label, errors) {
  if (typeof path !== "string" || path.length === 0) {
    addError(errors, `${label} must be a non-empty string`);
    return;
  }
  const requiredPrefix = `${EXPECTED_AUTHORITY.componentRoot}/`;
  if (!path.startsWith(requiredPrefix)) {
    addError(errors, `${label} must remain under ${requiredPrefix}`);
  }
  if (
    path.includes("\\") ||
    path.includes("\0") ||
    path.endsWith("/") ||
    posix.normalize(path) !== path
  ) {
    addError(errors, `${label} must be a normalized file path`);
  }
  if (globPattern.test(path)) {
    addError(errors, `${label} must be exact and cannot contain glob syntax`);
  }
}

function validateUniqueSortedPaths(paths, label, errors) {
  const seen = new Set();
  paths.forEach((path, index) => {
    validatePath(path, `${label}[${index}]`, errors);
    if (seen.has(path)) {
      addError(errors, `${label} contains duplicate path: ${String(path)}`);
    }
    seen.add(path);
    if (
      index > 0 &&
      typeof path === "string" &&
      typeof paths[index - 1] === "string" &&
      paths[index - 1] > path
    ) {
      addError(errors, `${label} must be sorted`);
    }
  });
  return seen;
}

function validateCounts(partitions, auditedPaths, errors) {
  for (const name of PARTITION_NAMES) {
    if (partitions[name].length !== EXPECTED_COUNTS[name]) {
      addError(
        errors,
        `${name} count must be ${EXPECTED_COUNTS[name]}, received ${partitions[name].length}`,
      );
    }
  }
  const partitionTotal = PARTITION_NAMES.reduce(
    (total, name) => total + partitions[name].length,
    0,
  );
  if (partitionTotal !== EXPECTED_COUNTS.total) {
    addError(
      errors,
      `partition total must be ${EXPECTED_COUNTS.total}, received ${partitionTotal}`,
    );
  }
  if (auditedPaths.length !== EXPECTED_COUNTS.total) {
    addError(
      errors,
      `audited path count must be ${EXPECTED_COUNTS.total}, received ${auditedPaths.length}`,
    );
  }
}

function partitionOwnerMap(partitions, errors) {
  const owners = new Map();
  for (const name of PARTITION_NAMES) {
    for (const path of partitions[name]) {
      const existing = owners.get(path);
      if (existing !== undefined) {
        addError(
          errors,
          `path appears in both ${existing} and ${name}: ${String(path)}`,
        );
      } else {
        owners.set(path, name);
      }
    }
  }
  return owners;
}

function compareCoverage(owners, auditedSet, errors) {
  for (const path of auditedSet) {
    if (!owners.has(path)) {
      addError(errors, `audited path is not classified: ${path}`);
    }
  }
  for (const path of owners.keys()) {
    if (!auditedSet.has(path)) {
      addError(
        errors,
        `classified path is not in the audited fixture: ${path}`,
      );
    }
  }
}

function validateComponentExclusions(componentExclusions, errors) {
  const actual = new Set(componentExclusions);
  for (const path of EXPECTED_COMPONENT_EXCLUSIONS) {
    if (!actual.has(path)) {
      addError(errors, `component-level exclusion is missing: ${path}`);
    }
  }
  for (const path of actual) {
    if (!EXPECTED_COMPONENT_EXCLUSIONS.includes(path)) {
      addError(errors, `unexpected component-level exclusion: ${String(path)}`);
    }
  }
}

function documentPaths(document) {
  if (typeof document !== "string") {
    return [];
  }
  const componentFile =
    /`(apps\/mobile\/modules\/expo-midnight-native\/[^`\r\n]+)`/gu;
  return [...document.matchAll(componentFile)].map((match) => match[1]);
}

function documentPartitionPaths(document, errors) {
  const candidatePattern =
    /^\|\s*(?:Copy|Select)\s*\|\s*`(apps\/mobile\/modules\/expo-midnight-native\/[^`\r\n]+)`\s*\|/gmu;
  const explicitHeading = "## Explicit exclusions";
  const landingHeading = "## Landing gates";
  const componentMarker = "The component-level rule denies exactly:";
  const explicitStart = document.indexOf(explicitHeading);
  const explicitEnd = document.indexOf(landingHeading, explicitStart);

  if (explicitStart < 0 || explicitEnd < 0) {
    addError(errors, "allowlist document exclusion section is missing");
    return {
      candidates: [],
      componentLevelExclusions: [],
      exactExclusions: [],
    };
  }

  const explicitSection = document.slice(explicitStart, explicitEnd);
  const componentStart = explicitSection.indexOf(componentMarker);
  if (componentStart < 0) {
    addError(
      errors,
      "allowlist document component exclusion marker is missing",
    );
    return {
      candidates: [...document.matchAll(candidatePattern)].map(
        (match) => match[1],
      ),
      componentLevelExclusions: [],
      exactExclusions: [],
    };
  }

  const bulletPattern =
    /^-\s+`(apps\/mobile\/modules\/expo-midnight-native\/[^`\r\n]+)`$/gmu;
  return {
    candidates: [...document.matchAll(candidatePattern)].map(
      (match) => match[1],
    ),
    exactExclusions: [
      ...explicitSection.slice(0, componentStart).matchAll(bulletPattern),
    ].map((match) => match[1]),
    componentLevelExclusions: [
      ...explicitSection.slice(componentStart).matchAll(bulletPattern),
    ].map((match) => match[1]),
  };
}

function validateDocumentPartition(actual, expected, label, errors) {
  const actualSet = new Set(actual);
  const expectedSet = new Set(expected);
  for (const path of expectedSet) {
    if (!actualSet.has(path)) {
      addError(errors, `allowlist document ${label} is missing: ${path}`);
    }
  }
  for (const path of actualSet) {
    if (!expectedSet.has(path)) {
      addError(errors, `allowlist document ${label} is unexpected: ${path}`);
    }
  }
}

function validateDocument(document, partitions, auditedSet, errors) {
  if (typeof document !== "string") {
    addError(errors, "allowlist document must be a string");
    return;
  }
  for (const expected of Object.values(EXPECTED_AUTHORITY)) {
    if (!document.includes(expected)) {
      addError(
        errors,
        `allowlist document is missing authority value: ${expected}`,
      );
    }
  }
  const paths = documentPaths(document);
  const declaredSet = validateUniqueSortedDocumentPaths(paths, errors);
  for (const path of auditedSet) {
    if (!declaredSet.has(path)) {
      addError(errors, `allowlist document is missing audited path: ${path}`);
    }
  }
  for (const path of declaredSet) {
    if (!auditedSet.has(path)) {
      addError(errors, `allowlist document has unaudited path: ${path}`);
    }
  }

  const documentedPartitions = documentPartitionPaths(document, errors);
  for (const name of PARTITION_NAMES) {
    validateDocumentPartition(
      documentedPartitions[name],
      partitions[name],
      name,
      errors,
    );
  }
}

function validateUniqueSortedDocumentPaths(paths, errors) {
  const seen = new Set();
  for (const path of paths) {
    if (seen.has(path)) {
      addError(errors, `allowlist document repeats path: ${path}`);
    }
    seen.add(path);
  }
  return seen;
}

function validateUniffiPath(partitions, auditedSet, document, errors) {
  if (!partitions.candidates.includes(REQUIRED_UNIFFI_PATH)) {
    addError(errors, `candidates must include ${REQUIRED_UNIFFI_PATH}`);
  }
  if (!auditedSet.has(REQUIRED_UNIFFI_PATH)) {
    addError(errors, `audited fixture must include ${REQUIRED_UNIFFI_PATH}`);
  }
  if (
    typeof document !== "string" ||
    !document.includes(`\`${REQUIRED_UNIFFI_PATH}\``)
  ) {
    addError(errors, `allowlist document must include ${REQUIRED_UNIFFI_PATH}`);
  }
  const allPartitionPaths = PARTITION_NAMES.flatMap((name) => partitions[name]);
  if (
    allPartitionPaths.includes(INCORRECT_UNIFFI_PATH) ||
    auditedSet.has(INCORRECT_UNIFFI_PATH) ||
    (typeof document === "string" && document.includes(INCORRECT_UNIFFI_PATH))
  ) {
    addError(
      errors,
      `obsolete UniFFI path is forbidden: ${INCORRECT_UNIFFI_PATH}`,
    );
  }
}

export function parseAuditedPaths(contents) {
  if (typeof contents !== "string") {
    return [];
  }
  const paths = contents.split(/\r\n|\n|\r/u);
  if (paths.at(-1) === "") {
    paths.pop();
  }
  return paths;
}

export function validateExtractionAllowlist({
  manifest,
  auditedPaths,
  document,
}) {
  const errors = [];
  validateAuthority(manifest, errors);

  const partitions = Object.fromEntries(
    PARTITION_NAMES.map((name) => [
      name,
      readStringArray(manifest, name, errors),
    ]),
  );
  const partitionSets = Object.fromEntries(
    PARTITION_NAMES.map((name) => [
      name,
      validateUniqueSortedPaths(partitions[name], name, errors),
    ]),
  );
  const normalizedAuditedPaths = Array.isArray(auditedPaths)
    ? auditedPaths
    : [];
  if (!Array.isArray(auditedPaths)) {
    addError(errors, "auditedPaths must be an array");
  }
  const auditedSet = validateUniqueSortedPaths(
    normalizedAuditedPaths,
    "auditedPaths",
    errors,
  );

  validateCounts(partitions, normalizedAuditedPaths, errors);
  const owners = partitionOwnerMap(partitions, errors);
  compareCoverage(owners, auditedSet, errors);
  validateComponentExclusions(partitionSets.componentLevelExclusions, errors);
  validateDocument(document, partitions, auditedSet, errors);
  validateUniffiPath(partitions, auditedSet, document, errors);
  return errors;
}

function loadInputs() {
  const manifest = JSON.parse(readFileSync(MANIFEST_PATH, "utf8"));
  const auditedPaths = parseAuditedPaths(
    readFileSync(AUDITED_PATHS_FIXTURE, "utf8"),
  );
  const document = readFileSync(ALLOWLIST_DOCUMENT, "utf8");
  return { manifest, auditedPaths, document };
}

function main() {
  let inputs;
  try {
    inputs = loadInputs();
  } catch (error) {
    const detail = error instanceof Error ? error.message : "unknown error";
    console.error(`unable to load extraction allowlist inputs: ${detail}`);
    process.exitCode = 1;
    return;
  }

  const errors = validateExtractionAllowlist(inputs);
  if (errors.length > 0) {
    errors.forEach((error) => console.error(error));
    process.exitCode = 1;
    return;
  }
  console.log(
    "validated extraction allowlist: 110 audited paths (69 candidates, 39 exact exclusions, 2 component-level exclusions)",
  );
}

const entryPath = process.argv[1];
if (
  entryPath !== undefined &&
  import.meta.url === pathToFileURL(resolve(entryPath)).href
) {
  main();
}
