import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  ALLOWED_COMMAND_KINDS,
  extractRustKinds,
  extractTypeScriptKindList,
  extractTypeScriptKinds,
  findForbiddenContent,
  validateCommandContracts,
  validatePolicyManifest,
} from "./check-wallet-core-boundary.mjs";
import { validateSanitizedTargetManifest } from "./check-wallet-core-targets.mjs";

const legacyTargetManifests = [
  "./m1-sanitized-target-manifest.json",
  "./m2-sanitized-target-manifest.json",
  "./m3-sanitized-target-manifest.json",
].map((path) =>
  JSON.parse(readFileSync(new URL(path, import.meta.url), "utf8")),
);
const targetManifest = JSON.parse(
  readFileSync(
    new URL("./m4-sanitized-target-manifest.json", import.meta.url),
    "utf8",
  ),
);
const manifest = JSON.parse(
  readFileSync(
    new URL("./wallet-core-boundary-manifest.json", import.meta.url),
    "utf8",
  ),
);

function title(value) {
  return value[0].toUpperCase() + value.slice(1);
}

function typescriptFixture(kinds = ALLOWED_COMMAND_KINDS) {
  const fields = kinds
    .map((kind) => `  readonly ${kind}: { readonly kind: "${kind}" };`)
    .join("\n");
  const results = kinds
    .map((kind) => `  readonly ${kind}: { readonly ok: true };`)
    .join("\n");
  const values = kinds.map((kind) => `  "${kind}",`).join("\n");
  return [
    "interface MidnightCommandMap {",
    fields,
    "}",
    "interface MidnightCommandResultMap {",
    results,
    "}",
    "export const MIDNIGHT_COMMAND_KINDS = [",
    values,
    "] as const;",
  ].join("\n");
}

function rustFixture(kinds = ALLOWED_COMMAND_KINDS) {
  const variants = kinds.map((kind) => `    ${title(kind)},`).join("\n");
  return [
    '#[serde(tag = "kind", rename_all = "camelCase")]',
    "enum RuntimeCommand {",
    variants,
    "}",
  ].join("\n");
}

function includesError(errors, fragment) {
  assert.ok(
    errors.some((error) => error.includes(fragment)),
    `expected error containing ${JSON.stringify(fragment)}:\n${errors.join("\n")}`,
  );
}

test("policy manifest fixes contracts, scan roots, and coverage gates", () => {
  assert.deepEqual(validatePolicyManifest(structuredClone(manifest)), []);
});

test("policy manifest rejects command, scan, issue, and expiry drift", () => {
  const value = structuredClone(manifest);
  value.commandKinds.pop();
  value.textInputs.pop();
  value.rustCoverageException.trackingIssue =
    "https://github.com/ADGLx/midnight-mobile/issues/39";
  value.typescriptCoverage.runner = "test-inclusive";
  value.typescriptCoverage.minimumPercent.functions = 64;
  value.currentMilestone = "M5";
  const errors = validatePolicyManifest(value);
  includesError(errors, "exact ordered 19-kind contract");
  includesError(errors, "all maintained/generated/package roots");
  includesError(errors, "issues/40");
  includesError(errors, "expired at M5");
  includesError(errors, "compiled-production-node-tests");
  includesError(errors, "minimumPercent.functions");
});

test("extracts all TypeScript and Rust command surfaces", () => {
  const ts = typescriptFixture();
  const rust = rustFixture();
  assert.deepEqual(
    extractTypeScriptKinds(ts, "MidnightCommandMap"),
    ALLOWED_COMMAND_KINDS,
  );
  assert.deepEqual(
    extractTypeScriptKinds(ts, "MidnightCommandResultMap"),
    ALLOWED_COMMAND_KINDS,
  );
  assert.deepEqual(extractTypeScriptKindList(ts), ALLOWED_COMMAND_KINDS);
  assert.deepEqual(extractRustKinds(rust), ALLOWED_COMMAND_KINDS);
  assert.deepEqual(
    validateCommandContracts({
      typescriptSource: ts,
      rustSource: rust,
    }),
    [],
  );
});

test("rejects added, removed, and reordered kinds in every contract", () => {
  const extra = [...ALLOWED_COMMAND_KINDS, "notWalletCore"];
  const removed = ALLOWED_COMMAND_KINDS.slice(1);
  const reordered = [...ALLOWED_COMMAND_KINDS].reverse();
  includesError(
    validateCommandContracts({
      typescriptSource: typescriptFixture(extra),
      rustSource: rustFixture(),
    }),
    "TypeScript command map",
  );
  includesError(
    validateCommandContracts({
      typescriptSource: typescriptFixture(),
      rustSource: rustFixture(removed),
    }),
    "Rust RuntimeCommand enum",
  );
  includesError(
    validateCommandContracts({
      typescriptSource: typescriptFixture(reordered),
      rustSource: rustFixture(),
    }),
    "TypeScript exported kind list",
  );
});

test("target manifest fixes schema and rejects target-set drift", () => {
  for (const legacy of legacyTargetManifests) {
    const legacyPaths = legacy.entries.map((entry) => entry.targetPath);
    assert.deepEqual(
      validateSanitizedTargetManifest(structuredClone(legacy), legacyPaths),
      [],
    );
  }
  const paths = targetManifest.entries.map((entry) => entry.targetPath);
  assert.deepEqual(
    validateSanitizedTargetManifest(structuredClone(targetManifest), paths),
    [],
  );
  const errors = validateSanitizedTargetManifest(
    structuredClone(targetManifest),
    [...paths.slice(1), "crates/runtime/src/untracked.rs"],
  );
  includesError(errors, "sanitized target is missing");
  includesError(errors, "untracked sanitized target");
});

test("rejects raw-input and generated-output directories in target data", () => {
  const directories = [
    ["_", "input"].join(""),
    [".", "declaration", "-", "build"].join(""),
    ["tar", "get"].join(""),
    ["bu", "ild"].join(""),
    ["staging", "-", "build"].join(""),
  ];
  for (const directory of directories) {
    const value = structuredClone(targetManifest);
    value.entries[0].targetPath = `crates/runtime/${directory}/Cargo.toml`;
    includesError(
      validateSanitizedTargetManifest(value),
      "contains output directory",
    );
  }
});

test("detects every excluded capability family without literal fixtures", () => {
  const samples = [
    ["identity-extension", ["publish", "Post"].join("")],
    ["token-extension", ["asset", "_", "mint"].join("")],
    ["operated-service-auth", ["sign", "Gateway", "Challenge"].join("")],
    ["accelerated-private-sync", ["fast", "Sync"].join("")],
    ["disclosure-upload", ["viewing", "Key"].join("")],
    ["private-service-route", ["private", "Endpoint"].join("")],
    [
      "operated-service-default",
      ["DEFAULT", "_", "INDEXER", "_", "URL"].join(""),
    ],
  ];
  for (const [family, contents] of samples) {
    const errors = findForbiddenContent([
      { path: "packages/react-native/src/sample.ts", contents },
    ]);
    includesError(errors, family);
  }
});

test("detects excluded module paths and accepts wallet-core text", () => {
  const firstPath = ["crates/runtime/src", ["so", "cial"].join(""), "mod.rs"];
  const secondPath = ["crates/runtime/src", ["as", "sets"].join(""), "mod.rs"];
  assert.equal(
    findForbiddenContent([
      { path: firstPath.join("/"), contents: "pub fn helper() {}" },
      { path: secondPath.join("/"), contents: "pub fn helper() {}" },
    ]).length,
    2,
  );
  assert.deepEqual(
    findForbiddenContent([
      {
        path: "packages/react-native/src/transport.ts",
        contents: "export interface Endpoints { readonly nodeUrl: string }",
      },
    ]),
    [],
  );
});
