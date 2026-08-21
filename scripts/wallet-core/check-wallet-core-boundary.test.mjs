import assert from "node:assert/strict";
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  ALLOWED_COMMAND_KINDS,
  extractRustKinds,
  extractTypeScriptKindList,
  extractTypeScriptKinds,
  findForbiddenContent,
  runBoundaryCheck,
  validateCommandContracts,
  validateErrorSurfaces,
  validatePolicyManifest,
} from "./check-wallet-core-boundary.mjs";

const manifest = JSON.parse(
  readFileSync(new URL("./boundary-manifest.json", import.meta.url), "utf8"),
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

test("policy manifest defines contracts, command kinds, and focused sources", () => {
  assert.deepEqual(validatePolicyManifest(structuredClone(manifest)), []);
});

test("policy manifest rejects malformed, duplicate, and unsafe values", () => {
  const value = structuredClone(manifest);
  value.schemaVersion = 1;
  value.commandKinds.push(value.commandKinds[0]);
  value.sourceInputs = ["../outside"];
  delete value.contracts.rust;
  const errors = validatePolicyManifest(value);
  includesError(errors, "schemaVersion must be 2");
  includesError(errors, "duplicate kind");
  includesError(errors, "invalid repository path");
  includesError(errors, "exactly rust and typescript");
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

test("detects excluded capability text without rejecting generic asset paths", () => {
  const samples = [
    ["identity-extension", ["publish", "Post"].join("")],
    ["token-extension", ["asset", "_", "mint"].join("")],
    ["operated-service-auth", ["sign", "Gateway", "Challenge"].join("")],
    ["accelerated-private-sync", ["fast", "Sync"].join("")],
    ["disclosure-upload", ["upload", "ViewingKey"].join("")],
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
  assert.deepEqual(
    findForbiddenContent([
      {
        path: "examples/expo/assets/local-prover/parameters.json",
        contents: "public proving parameters",
      },
    ]),
    [],
  );
});

/**
 * The error-surface check reads fixed paths relative to the repository root, so
 * a fixture root has to provide the enum and both bridges to be checkable.
 */
function writeErrorSurfaceFixtures(
  root,
  variants = ["Unavailable", "SyncGap"],
) {
  mkdirSync(join(root, "crates/runtime/src"), { recursive: true });
  writeFileSync(
    join(root, "crates/runtime/src/lib.rs"),
    `pub enum MidnightRuntimeError {\n${variants
      .map((variant) => `    ${variant},`)
      .join("\n")}\n}\n`,
  );
  const bridges = [
    "packages/react-native/ios/MidnightMobileRuntimeModule.swift",
    "packages/react-native/android/src/main/java/dev/oneam/midnightmobile/MidnightMobileRuntimeModule.kt",
  ];
  for (const bridge of bridges) {
    mkdirSync(join(root, bridge, ".."), { recursive: true });
    const source = bridge.endsWith(".swift")
      ? variants
          .map((variant) => `case MidnightRuntimeError.${variant}:`)
          .join("\n")
      : variants
          .map((variant) => `is MidnightRuntimeException.${variant} ->`)
          .join("\n");
    writeFileSync(join(root, bridge), source);
  }
  return bridges;
}

test("every runtime error variant must be named in both platform bridges", () => {
  const enumSource =
    "pub enum MidnightRuntimeError {\n    Unavailable,\n    SyncGap,\n}\n";
  assert.deepEqual(
    validateErrorSurfaces({
      errorEnumSource: enumSource,
      surfaces: {
        "ios.swift":
          "case MidnightRuntimeError.Unavailable:\ncase MidnightRuntimeError.SyncGap:",
        "android.kt":
          "is MidnightRuntimeException.Unavailable ->\nis MidnightRuntimeException.SyncGap ->",
      },
    }),
    [],
  );
  // A bridge that omits a variant reports it as NATIVE_INTERNAL, which is the
  // failure this check exists to prevent.
  includesError(
    validateErrorSurfaces({
      errorEnumSource: enumSource,
      surfaces: {
        "ios.swift": "case MidnightRuntimeError.Unavailable:",
        "android.kt":
          "is MidnightRuntimeException.Unavailable ->\nis MidnightRuntimeException.SyncGap ->",
      },
    }),
    "ios.swift does not surface the SyncGap runtime error",
  );
  // Do not count text in comments or strings as switch arms.
  includesError(
    validateErrorSurfaces({
      errorEnumSource: enumSource,
      surfaces: {
        "ios.swift":
          '/*\ncase MidnightRuntimeError.Unavailable:\n*/\nlet value = "case MidnightRuntimeError.SyncGap:"',
        "android.kt":
          '// is MidnightRuntimeException.Unavailable ->\nval value = "is MidnightRuntimeException.SyncGap ->"',
      },
    }),
    "ios.swift does not surface the Unavailable runtime error",
  );
  includesError(
    validateErrorSurfaces({
      errorEnumSource: "pub enum Other {}",
      surfaces: {},
    }),
    "MidnightRuntimeError enum was not found",
  );
});

test("boundary scans configured source trees without inventorying every file", () => {
  const root = mkdtempSync(join(tmpdir(), "midnight-boundary-test-"));
  try {
    const source = join(root, "src");
    mkdirSync(source, { recursive: true });
    writeFileSync(join(source, "commands.ts"), typescriptFixture());
    writeFileSync(join(source, "types.rs"), rustFixture());
    writeFileSync(join(source, "new-helper.ts"), "export const value = 1;\n");
    writeErrorSurfaceFixtures(root);
    mkdirSync(join(root, "examples/assets"), { recursive: true });
    writeFileSync(
      join(root, "examples/assets/ignored.json"),
      '{"value":"publishPost"}\n',
    );
    const fixtureManifest = {
      schemaVersion: 2,
      contracts: {
        typescript: "src/commands.ts",
        rust: "src/types.rs",
      },
      commandKinds: [...ALLOWED_COMMAND_KINDS],
      sourceInputs: ["src"],
    };
    const manifestPath = join(root, "boundary.json");
    writeFileSync(
      manifestPath,
      `${JSON.stringify(fixtureManifest, null, 2)}\n`,
    );

    const first = runBoundaryCheck({ repositoryRoot: root, manifestPath });
    assert.deepEqual(first.errors, []);
    assert.ok(first.files.some((file) => file.path === "src/new-helper.ts"));

    writeFileSync(join(source, "forbidden.ts"), "const x = 'publishPost';\n");
    includesError(
      runBoundaryCheck({ repositoryRoot: root, manifestPath }).errors,
      "identity-extension",
    );

    fixtureManifest.sourceInputs.push("missing-source");
    writeFileSync(
      manifestPath,
      `${JSON.stringify(fixtureManifest, null, 2)}\n`,
    );
    includesError(
      runBoundaryCheck({ repositoryRoot: root, manifestPath }).errors,
      "source input is missing",
    );
  } finally {
    rmSync(root, { force: true, recursive: true });
  }
});
