import assert from "node:assert/strict";
import { existsSync, mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import test from "node:test";

import {
  extractAsciiStrings,
  findForbiddenBinaryContent,
  findPublicDeclarationErrors,
  findStagingPackageErrors,
  resetGeneratedDirectory,
  selectRuntimeArtifacts,
} from "./check-wallet-core-artifacts.mjs";
import { ALLOWED_COMMAND_KINDS } from "./check-wallet-core-boundary.mjs";

function declaration(resultType = "{ readonly ok: true }") {
  const fields = ALLOWED_COMMAND_KINDS.map(
    (kind) => `  readonly ${kind}: ${resultType};`,
  ).join("\n");
  return `export interface MidnightCommandResultMap {\n${fields}\n}\n`;
}

await test("extracts printable binary strings without crossing separators", () => {
  const bytes = new Uint8Array([
    ...new TextEncoder().encode("wallet"),
    0,
    ...new TextEncoder().encode("core"),
    1,
    0x61,
  ]);
  assert.deepEqual(extractAsciiStrings(bytes), ["wallet", "core"]);
});

await test("rejects unknown and drifted public command result declarations", () => {
  assert.deepEqual(findPublicDeclarationErrors(declaration()), []);
  assert.match(
    findPublicDeclarationErrors(declaration("unknown")).join("\n"),
    /must not contain unknown/u,
  );
  assert.match(
    findPublicDeclarationErrors(
      declaration().replace(/^\s+readonly signData:.*\n/mu, ""),
    ).join("\n"),
    /exactly 19 kinds/u,
  );
});

await test("applies forbidden capability rules to binary string tables", () => {
  const forbidden = ["sign", "Gateway", "Challenge"].join("");
  const bytes = new TextEncoder().encode(`safe\0${forbidden}\0wallet`);
  assert.match(
    findForbiddenBinaryContent("target/debug/runtime.rlib", bytes).join("\n"),
    /operated-service-auth/u,
  );
  assert.deepEqual(
    findForbiddenBinaryContent(
      "target/debug/runtime.rlib",
      new TextEncoder().encode("wallet\0runtime"),
    ),
    [],
  );
});

await test("staging package validation requires compiled entrypoints", () => {
  assert.deepEqual(
    findStagingPackageErrors([
      { path: "dist/index.js" },
      { path: "dist/index.d.ts" },
      {
        path: "android/src/main/jniLibs/arm64-v8a/libmidnight_native_runtime.so",
      },
      {
        path: "ios/build/MidnightNativeRuntime.xcframework/ios-arm64/MidnightNativeRuntime.framework/MidnightNativeRuntime",
      },
    ]),
    [],
  );
  assert.match(
    findStagingPackageErrors([]).join("\n"),
    /missing dist\/index\.js/u,
  );
  assert.match(
    findStagingPackageErrors([
      { path: "dist/index.js" },
      { path: "dist/index.d.ts" },
      { path: "ios/build/runtime.xcframework/runtime" },
    ]).join("\n"),
    /outside the M4 allowlist/u,
  );
});

await test("host artifact selection fails closed when none are available", () => {
  assert.deepEqual(selectRuntimeArtifacts([]), []);
  assert.deepEqual(
    selectRuntimeArtifacts([
      "/repo/target/debug/deps/libmidnight_native_runtime-abc.dylib",
      "/repo/target/debug/libmidnight_native_runtime.dylib",
      "/repo/target/debug/libmidnight_native_runtime.rlib",
    ]),
    ["/repo/target/debug/libmidnight_native_runtime.dylib"],
  );
});

await test("generated output reset removes stale nested artifacts", () => {
  const root = mkdtempSync(resolve(tmpdir(), "midnight-artifact-test-"));
  const generated = resolve(root, ".staging-build");
  mkdirSync(resolve(generated, "src"), { recursive: true });
  writeFileSync(resolve(generated, "src/stale.js"), "stale");
  resetGeneratedDirectory(generated);
  assert.equal(existsSync(generated), false);
  resetGeneratedDirectory(root);
});
