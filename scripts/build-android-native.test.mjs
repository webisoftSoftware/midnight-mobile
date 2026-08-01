import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  parseElfReport,
  validateElfReport,
  validateNativeBuildConfig,
} from "./build-android-native.mjs";

const config = JSON.parse(
  readFileSync(new URL("./native-build-config.json", import.meta.url), "utf8"),
);

await test("native build config pins the exact M4 Android contract", () => {
  assert.deepEqual(validateNativeBuildConfig(config), []);
  assert.match(
    validateNativeBuildConfig({
      ...config,
      android: { ...config.android, ndkVersion: "latest" },
    }).join("\n"),
    /27\.1\.12297006/u,
  );
  assert.match(
    validateNativeBuildConfig({
      ...config,
      rust: { ...config.rust, libraryBaseName: "midnight_runtime" },
    }).join("\n"),
    /library must be midnight_mobile_runtime/u,
  );
  assert.match(
    validateNativeBuildConfig({
      ...config,
      apple: { ...config.apple, moduleName: "MidnightMobileRuntimeFFI" },
    }).join("\n"),
    /framework and generated module names must match/u,
  );
});

await test("ELF inspection requires the right ABI and eight functions", () => {
  const symbols = config.rust.uniffiFunctions
    .map(
      (name) =>
        `0000000000000000 T uniffi_midnight_mobile_runtime_fn_func_${name}`,
    )
    .concat(config.android.localProver.exportedFunctions)
    .join("\n");
  const report = parseElfReport(
    "  Type: DYN (Shared object file)\n  Machine: AArch64\n",
    [
      "Shared library: [libdl.so]",
      "Shared library: [libm.so]",
      "Shared library: [libc.so]",
    ].join("\n"),
    symbols,
  );
  assert.deepEqual(
    validateElfReport(report, config.android.targets[0], config),
    [],
  );
  assert.match(
    validateElfReport(
      { ...report, functions: report.functions.slice(1) },
      config.android.targets[0],
      config,
    ).join("\n"),
    /function ABI drifted/u,
  );
  assert.match(
    validateElfReport(
      { ...report, localProverFunctions: report.localProverFunctions.slice(1) },
      config.android.targets[0],
      config,
    ).join("\n"),
    /local prover C ABI drifted/u,
  );
});
