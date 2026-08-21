import {
  existsSync,
  lstatSync,
  readFileSync,
  readdirSync,
  realpathSync,
} from "node:fs";
import { extname, relative, resolve } from "node:path";
import { pathToFileURL } from "node:url";

import { validatePolicyManifest } from "./wallet-core-policy.mjs";

const DEFAULT_MANIFEST = JSON.parse(
  readFileSync(new URL("./boundary-manifest.json", import.meta.url), "utf8"),
);
export const ALLOWED_COMMAND_KINDS = Object.freeze([
  ...DEFAULT_MANIFEST.commandKinds,
]);
export { validatePolicyManifest };

/** Where the runtime declares its error variants. */
const ERROR_ENUM_SOURCE = "crates/runtime/src/lib.rs";

/**
 * The bridges that translate the enum, and therefore have to name every variant.
 * The TypeScript union is deliberately not listed: it is a curated host surface
 * that folds some variants into a broader code, whereas a bridge that misses a
 * variant reports it as NATIVE_INTERNAL and loses it.
 */
const ERROR_SURFACES = Object.freeze([
  "packages/react-native/ios/MidnightMobileRuntimeModule.swift",
  "packages/react-native/android/src/main/java/dev/oneam/midnightmobile/MidnightMobileRuntimeModule.kt",
]);

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
      joined(
        "\\b(?:upload",
        "ViewingKey|viewingKeyUpload|viewing_key_upload|viewing-key-upload)\\b",
      ),
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

export function validateCommandContracts({
  typescriptSource,
  rustSource,
  commandKinds = ALLOWED_COMMAND_KINDS,
}) {
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
    if (!sameArray(kinds, commandKinds)) {
      errors.push(
        `${name} must contain the exact ordered ${String(commandKinds.length)} kinds; received ${JSON.stringify(kinds)}`,
      );
    }
  }
  return errors;
}

export function findForbiddenContent(files) {
  const errors = [];
  for (const file of files) {
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

/**
 * Both platform bridges turn the Rust error enum into a string code with a
 * switch that falls back to NATIVE_INTERNAL, so neither compiler can catch a
 * missing variant: a new typed error would reach the host as a generic internal
 * failure. Require every variant to be named in both bridges.
 */
export function validateErrorSurfaces({ errorEnumSource, surfaces }) {
  const block = errorEnumSource.match(
    /pub enum MidnightRuntimeError \{([\s\S]*?)\n\}/u,
  );
  if (!block)
    return ["MidnightRuntimeError enum was not found in the Rust runtime"];
  const variants = [...block[1].matchAll(/^\s{4}([A-Z][A-Za-z0-9]*),$/gmu)].map(
    (match) => match[1],
  );
  if (variants.length === 0)
    return ["MidnightRuntimeError declares no variants"];
  const errors = [];
  for (const [name, source] of Object.entries(surfaces)) {
    // Match only switch arms in code. Text in a comment or string is not a
    // mapping.
    const code = source
      .replace(/\/\*[\s\S]*?\*\//gu, "")
      .replace(/\/\/[^\n]*$/gmu, "")
      .replace(/"""[\s\S]*?"""/gu, "")
      .replace(/"(?:\\.|[^"\\])*"/gu, "");
    const cases = new Set(
      [
        ...code.matchAll(
          /^\s*case\s+MidnightRuntimeError\.([A-Z][A-Za-z0-9]*)\s*:/gmu,
        ),
        ...code.matchAll(
          /^\s*is\s+MidnightRuntimeException\.([A-Z][A-Za-z0-9]*)\s*->/gmu,
        ),
      ].map((match) => match[1]),
    );
    for (const variant of variants) {
      if (!cases.has(variant)) {
        errors.push(`${name} does not surface the ${variant} runtime error`);
      }
    }
  }
  return errors;
}

export function runBoundaryCheck({
  repositoryRoot = process.cwd(),
  manifestPath = resolve(
    repositoryRoot,
    "scripts/wallet-core/boundary-manifest.json",
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
  errors.push(
    ...validateCommandContracts({
      typescriptSource,
      rustSource,
      commandKinds: manifest.commandKinds,
    }),
  );

  const files = new Map();
  for (const input of manifest.sourceInputs) {
    const path = resolve(repositoryRoot, input);
    if (!existsSync(path)) {
      errors.push(`source input is missing: ${input}`);
      continue;
    }
    collectFiles(path, repositoryRoot, files, new Set());
  }
  const scannedFiles = [...files.values()].sort((left, right) =>
    left.path.localeCompare(right.path),
  );
  errors.push(...findForbiddenContent(scannedFiles));

  const errorSurfaces = {};
  for (const surface of ERROR_SURFACES) {
    const path = resolve(repositoryRoot, surface);
    if (!existsSync(path)) {
      errors.push(`error surface is missing: ${surface}`);
      continue;
    }
    errorSurfaces[surface] = readFileSync(path, "utf8");
  }
  const errorEnumPath = resolve(repositoryRoot, ERROR_ENUM_SOURCE);
  if (existsSync(errorEnumPath)) {
    errors.push(
      ...validateErrorSurfaces({
        errorEnumSource: readFileSync(errorEnumPath, "utf8"),
        surfaces: errorSurfaces,
      }),
    );
  } else {
    errors.push(`error enum source is missing: ${ERROR_ENUM_SOURCE}`);
  }
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
  console.log(
    `wallet-core boundary passed: ${String(result.manifest.commandKinds.length)} command kinds; ${String(result.files.length)} product source files scanned`,
  );
}

const invokedPath = process.argv[1];
if (
  invokedPath !== undefined &&
  pathToFileURL(resolve(invokedPath)).href === import.meta.url
) {
  main();
}
