import {
  existsSync,
  lstatSync,
  readFileSync,
  readdirSync,
  realpathSync,
} from "node:fs";
import { extname, relative, resolve } from "node:path";
import { pathToFileURL } from "node:url";

import {
  ALLOWED_COMMAND_KINDS,
  MILESTONES,
  validatePolicyManifest,
} from "./check-wallet-core-policy.mjs";
import {
  listSanitizedTargets,
  validateSanitizedTargetManifest,
} from "./check-wallet-core-targets.mjs";

export { ALLOWED_COMMAND_KINDS, validatePolicyManifest };

const TEXT_EXTENSIONS = new Set(
  ",.c,.cc,.cpp,.gradle,.h,.hpp,.java,.js,.json,.kt,.kts,.md,.mjs,.modulemap,.podspec,.properties,.rs,.sh,.swift,.toml,.ts,.tsx,.txt,.xml".split(
    ",",
  ),
);

function joined(...parts) {
  return parts.join("");
}

const CONTENT_RULES = Object.freeze([
  {
    name: "identity-extension",
    pattern: new RegExp(
      joined(
        "\\b(?:",
        "Social[A-Z]\\w*|social_[a-z]\\w*|publish",
        "Post|upsert",
        "Profile|set",
        "Reaction|update",
        "Relationship|set",
        "AccountMode)\\b|",
        "1",
        "am:",
      ),
      "u",
    ),
  },
  {
    name: "token-extension",
    pattern: new RegExp(
      joined(
        "\\b(?:Asset(?:Mint|Nft|Verifier)\\w*|asset_(?:mint|nft|verifier)\\w*|",
        "Nft",
        "Holdings|personal_",
        "mint|ft_",
        "launch)\\b",
      ),
      "u",
    ),
  },
  {
    name: "operated-service-auth",
    pattern: new RegExp(
      joined(
        "\\b(?:sign",
        "GatewayChallenge|gateway_(?:session|credential)|",
        "Gateway(?:Session|Credential|Challenge))\\b",
      ),
      "u",
    ),
  },
  {
    name: "accelerated-private-sync",
    pattern: new RegExp(
      joined("\\b(?:fast", "(?:Sync|_sync|-sync))\\b|/", "chain-data/v2\\b"),
      "u",
    ),
  },
  {
    name: "disclosure-upload",
    pattern: new RegExp(
      joined("\\b(?:viewing", "Key|viewing_key|viewing-key|ViewingKey)\\b"),
      "u",
    ),
  },
  {
    name: "private-service-route",
    pattern: new RegExp(
      joined(
        "\\b(?:private",
        "(?:Route|_route|-route|Endpoint)|authenticated",
        "(?:Sync|_sync))\\b",
      ),
      "u",
    ),
  },
  {
    name: "operated-service-default",
    pattern: new RegExp(
      joined(
        "\\b(?:DEFAULT_(?:INDEXER|PROOF|NODE)_(?:URL|ENDPOINT)|default",
        "(?:Indexer|ProofServer?|Node)(?:Url|Endpoint))\\b",
      ),
      "u",
    ),
  },
]);
const PATH_RULES = Object.freeze([
  {
    name: "identity-extension-path",
    pattern: new RegExp(joined("(?:^|/)", "social", "(?:/|\\.)"), "iu"),
  },
  {
    name: "token-extension-path",
    pattern: new RegExp(joined("(?:^|/)", "assets?", "(?:/|\\.)"), "iu"),
  },
]);

function sameArray(actual, expected) {
  return (
    Array.isArray(actual) &&
    actual.length === expected.length &&
    actual.every((value, index) => value === expected[index])
  );
}

function blockAfter(source, marker) {
  const start = source.indexOf(marker);
  if (start < 0) {
    return undefined;
  }
  const open = source.indexOf("{", start + marker.length);
  if (open < 0) {
    return undefined;
  }
  let depth = 0;
  for (let index = open; index < source.length; index += 1) {
    if (source[index] === "{") depth += 1;
    if (source[index] === "}") depth -= 1;
    if (depth === 0) return source.slice(open + 1, index);
  }
  return undefined;
}

export function extractTypeScriptKinds(source, interfaceName) {
  const body = blockAfter(source, `interface ${interfaceName}`);
  if (body === undefined) return [];
  return [...body.matchAll(/^\s{2}readonly ([A-Za-z][A-Za-z0-9]*):/gmu)].map(
    (match) => match[1],
  );
}

export function extractTypeScriptKindList(source) {
  const match = /MIDNIGHT_COMMAND_KINDS\s*=\s*\[([\s\S]*?)\]\s*as const/u.exec(
    source,
  );
  if (match === null) return [];
  return [...match[1].matchAll(/"([A-Za-z][A-Za-z0-9]*)"/gu)].map(
    (item) => item[1],
  );
}

function upperCamelToLowerCamel(value) {
  const prefix = /^([A-Z]+)(?=[A-Z][a-z]|\d|\b)/u.exec(value)?.[1];
  return prefix === undefined
    ? value[0].toLowerCase() + value.slice(1)
    : prefix.toLowerCase() + value.slice(prefix.length);
}

export function extractRustKinds(source) {
  const body = blockAfter(source, "enum RuntimeCommand");
  if (body === undefined) return [];
  return [...body.matchAll(/^\s{4}([A-Z][A-Za-z0-9]*)\s*(?:\{|,)/gmu)].map(
    (match) => upperCamelToLowerCamel(match[1]),
  );
}

export function validateCommandContracts({ typescriptSource, rustSource }) {
  const errors = [];
  const surfaces = [
    [
      "TypeScript command map",
      extractTypeScriptKinds(typescriptSource, "MidnightCommandMap"),
    ],
    [
      "TypeScript result map",
      extractTypeScriptKinds(typescriptSource, "MidnightCommandResultMap"),
    ],
    [
      "TypeScript exported kind list",
      extractTypeScriptKindList(typescriptSource),
    ],
    ["Rust RuntimeCommand enum", extractRustKinds(rustSource)],
  ];
  for (const [name, kinds] of surfaces) {
    if (!sameArray(kinds, ALLOWED_COMMAND_KINDS)) {
      errors.push(
        `${name} must contain the exact ordered 19 kinds; received ${JSON.stringify(kinds)}`,
      );
    }
  }
  return errors;
}

export function findForbiddenContent(files) {
  const errors = [];
  for (const file of files) {
    for (const rule of PATH_RULES) {
      if (rule.pattern.test(file.path)) {
        errors.push(`${file.path}: excluded capability path (${rule.name})`);
      }
    }
    for (const rule of CONTENT_RULES) {
      if (rule.pattern.test(file.contents)) {
        errors.push(`${file.path}: excluded capability text (${rule.name})`);
      }
    }
  }
  return errors;
}

function isTextFile(path) {
  return TEXT_EXTENSIONS.has(extname(path).toLowerCase());
}

function collectFiles(path, repositoryRoot, output, visited) {
  const stat = lstatSync(path);
  if (stat.isSymbolicLink()) {
    const real = realpathSync(path);
    if (visited.has(real)) return;
    visited.add(real);
    collectFiles(real, repositoryRoot, output, visited);
  } else if (stat.isDirectory()) {
    for (const entry of readdirSync(path, { withFileTypes: true })) {
      collectFiles(resolve(path, entry.name), repositoryRoot, output, visited);
    }
  } else if (stat.isFile() && isTextFile(path)) {
    output.set(realpathSync(path), {
      path: relative(repositoryRoot, path).replaceAll("\\", "/"),
      contents: readFileSync(path, "utf8"),
    });
  }
}

function readJson(path, errors, label) {
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    const detail = error instanceof Error ? error.message : "unknown error";
    errors.push(`${label}: unable to read JSON: ${detail}`);
    return {};
  }
}

export function runBoundaryCheck({
  repositoryRoot = process.cwd(),
  manifestPath = resolve(
    repositoryRoot,
    "scripts/wallet-core-boundary-manifest.json",
  ),
} = {}) {
  const errors = [];
  const manifest = readJson(manifestPath, errors, "boundary manifest");
  errors.push(...validatePolicyManifest(manifest));
  if (errors.length > 0) return { errors, files: [], manifest };

  const contract = (name) => resolve(repositoryRoot, manifest.contracts[name]);
  let typescriptSource = "";
  let rustSource = "";
  try {
    typescriptSource = readFileSync(contract("typescript"), "utf8");
    rustSource = readFileSync(contract("rust"), "utf8");
  } catch (error) {
    const detail = error instanceof Error ? error.message : "unknown error";
    errors.push(`unable to read command contracts: ${detail}`);
  }
  errors.push(...validateCommandContracts({ typescriptSource, rustSource }));

  const targetManifest = readJson(
    contract("extractionManifest"),
    errors,
    "sanitized target manifest",
  );
  try {
    const actualTargets = listSanitizedTargets(repositoryRoot, [
      ...(targetManifest.targetFiles ?? []),
      ...(targetManifest.targetRoots ?? []),
    ]);
    errors.push(
      ...validateSanitizedTargetManifest(targetManifest, actualTargets),
    );
  } catch (error) {
    const detail = error instanceof Error ? error.message : "unknown error";
    errors.push(`unable to enumerate sanitized targets: ${detail}`);
  }

  const files = new Map();
  const current = MILESTONES.indexOf(manifest.currentMilestone);
  for (const input of manifest.textInputs) {
    const path = resolve(repositoryRoot, input.path);
    if (!existsSync(path)) {
      const required = MILESTONES.indexOf(input.requiredAtMilestone);
      if (input.requiredAtMilestone !== null && current >= required) {
        errors.push(`${input.group} text input is missing: ${input.path}`);
      }
      continue;
    }
    collectFiles(path, repositoryRoot, files, new Set());
  }
  const scannedFiles = [...files.values()].sort((left, right) =>
    left.path.localeCompare(right.path),
  );
  errors.push(...findForbiddenContent(scannedFiles));
  return { errors, files: scannedFiles, manifest };
}

function main() {
  const rootIndex = process.argv.indexOf("--root");
  const repositoryRoot =
    rootIndex >= 0 && process.argv[rootIndex + 1] !== undefined
      ? resolve(process.argv[rootIndex + 1])
      : process.cwd();
  const result = runBoundaryCheck({ repositoryRoot });
  if (result.errors.length > 0) {
    result.errors.sort().forEach((error) => console.error(error));
    process.exitCode = 1;
    return;
  }
  const groups = new Map(
    ["maintained", "generated", "package"].map((name) => [name, 0]),
  );
  for (const file of result.files) {
    const input = result.manifest.textInputs.find(
      (candidate) =>
        file.path === candidate.path ||
        file.path.startsWith(`${candidate.path}/`),
    );
    if (input !== undefined) {
      groups.set(input.group, (groups.get(input.group) ?? 0) + 1);
    }
  }
  console.log(
    `wallet-core boundary passed: 19 command kinds; ${result.files.length} text files scanned`,
  );
  console.log(
    `scan groups: maintained=${groups.get("maintained")}, generated=${groups.get("generated")}, package=${groups.get("package")}`,
  );
  console.log(
    "temporary Rust line-coverage exception active for M1-M4; expires at M5; tracking issue #7",
  );
  console.log(
    "temporary TypeScript coverage exception active for M1-M2; measured 71.30/73.23/63.19; expires at M3; tracking issue #16",
  );
}

const invokedPath = process.argv[1];
if (
  invokedPath !== undefined &&
  pathToFileURL(resolve(invokedPath)).href === import.meta.url
) {
  main();
}
