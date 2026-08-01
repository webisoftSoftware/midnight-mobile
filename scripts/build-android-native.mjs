import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { basename, resolve } from "node:path";
import { pathToFileURL } from "node:url";

import { findForbiddenBinaryContent } from "./check-wallet-core-artifacts.mjs";
import { removeTree } from "./quality-utils.mjs";

const CONFIG_PATH = "scripts/native-build-config.json";
const PACKAGE_ROOT = "packages/react-native";

function isObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function commandError(label, result) {
  const detail = `${result.stderr ?? result.stdout ?? ""}`.trim();
  return `${label} failed${detail.length === 0 ? "" : `:\n${detail}`}`;
}

function run(repositoryRoot, command, arguments_, options = {}) {
  return spawnSync(command, arguments_, {
    cwd: repositoryRoot,
    encoding: "utf8",
    maxBuffer: 30 * 1024 * 1024,
    ...options,
  });
}

function runChecked(repositoryRoot, command, arguments_, label, options = {}) {
  const result = run(repositoryRoot, command, arguments_, options);
  if (result.status !== 0) throw new Error(commandError(label, result));
  return result;
}

function sameArray(actual, expected) {
  return (
    Array.isArray(actual) &&
    actual.length === expected.length &&
    actual.every((value, index) => value === expected[index])
  );
}

export function validateNativeBuildConfig(config) {
  if (!isObject(config) || config.schemaVersion !== 1) {
    return ["native build config schemaVersion must be 1"];
  }
  const errors = [];
  if (config.rust?.toolchain !== "1.97.1") {
    errors.push("Rust toolchain must be pinned to 1.97.1");
  }
  if (config.rust?.package !== "midnight-mobile-runtime") {
    errors.push("Rust package must be midnight-mobile-runtime");
  }
  if (config.rust?.libraryBaseName !== "midnight_mobile_runtime") {
    errors.push("Rust library must be midnight_mobile_runtime");
  }
  const functions =
    "apply_sync_batch,begin_command,cancel_operation,close_wallet_session,export_wallet_checkpoint,get_wallet_snapshot,open_wallet_session,resume_operation".split(
      ",",
    );
  if (!sameArray(config.rust?.uniffiFunctions, functions)) {
    errors.push(
      "UniFFI functions must retain the exact ordered eight-function ABI",
    );
  }
  validateAppleConfig(config.apple, errors);
  validateAndroidConfig(config, errors);
  return errors;
}

function validateAppleConfig(apple, errors) {
  if (apple?.deploymentTarget !== "15.1") {
    errors.push("Apple deployment target must be iOS 15.1");
  }
  if (
    apple?.frameworkName !== "MidnightMobileRuntime" ||
    apple?.moduleName !== "MidnightMobileRuntime"
  ) {
    errors.push("Apple framework and generated module names must match");
  }
  if (
    apple?.installName !==
    "@rpath/MidnightMobileRuntime.framework/MidnightMobileRuntime"
  ) {
    errors.push("Apple dynamic install name must remain pinned");
  }
  const targets = Array.isArray(apple?.targets) ? apple.targets : [];
  const triples = targets.map((target) => [
    target?.rustTarget,
    target?.sdk,
    target?.architecture,
  ]);
  const expected = [
    ["aarch64-apple-ios", "iphoneos", "arm64"],
    ["aarch64-apple-ios-sim", "iphonesimulator", "arm64"],
    ["x86_64-apple-ios", "iphonesimulator", "x86_64"],
  ];
  if (
    triples.length !== expected.length ||
    triples.some((triple, index) => !sameArray(triple, expected[index]))
  ) {
    errors.push("Apple targets must match the reviewed architecture set");
  }
  if (apple?.compressedBudgetBytes !== 15 * 1024 * 1024) {
    errors.push("Apple compressed XCFramework budget must be exactly 15 MiB");
  }
}

function validateAndroidConfig(config, errors) {
  const android = config.android;
  if (android?.ndkVersion !== "27.1.12297006") {
    errors.push("Android NDK must be pinned to 27.1.12297006");
  }
  if (android?.apiLevel !== 24) errors.push("Android API level must be 24");
  const targets = Array.isArray(android?.targets) ? android.targets : [];
  const pairs = targets.map((target) => [
    target?.rustTarget,
    target?.abi,
    target?.machine,
    target?.clangPrefix,
  ]);
  const expectedPairs = [
    ["aarch64-linux-android", "arm64-v8a", "AArch64", "aarch64-linux-android"],
    [
      "x86_64-linux-android",
      "x86_64",
      "Advanced Micro Devices X86-64",
      "x86_64-linux-android",
    ],
  ];
  if (
    pairs.length !== expectedPairs.length ||
    pairs.some((pair, index) => !sameArray(pair, expectedPairs[index]))
  ) {
    errors.push("Android targets must be the exact arm64-v8a/x86_64 pair");
  }
  validateAndroidDependencies(android, errors);
}

function validateAndroidDependencies(android, errors) {
  if (
    !sameArray(android?.neededLibraries, ["libc.so", "libdl.so", "libm.so"])
  ) {
    errors.push("Android dependencies must be the exact reviewed system set");
  }
  if (android?.jna?.coordinate !== "net.java.dev.jna:jna:5.17.0@aar") {
    errors.push("Android JNA dependency must be the exact 5.17.0 AAR");
  }
  if (
    android?.jna?.sha256 !==
    "4dbeffffa665d97ad5aa7eee297531d3c841a86716ab7f774fd6956422b3cf38"
  ) {
    errors.push("Android JNA 5.17.0 AAR checksum must remain pinned");
  }
  if (android?.combinedBudgetBytes !== 30 * 1024 * 1024) {
    errors.push("Android native payload budget must be exactly 30 MiB");
  }
  validateAndroidLocalProver(android?.localProver, errors);
}

function validateAndroidLocalProver(localProver, errors) {
  if (localProver?.enabled !== true) {
    errors.push("Android local prover must be explicitly enabled");
  }
  if (!sameArray(localProver?.cargoFeatures, ["local-prover"])) {
    errors.push(
      "Android local prover Cargo feature must be exactly local-prover",
    );
  }
  if (
    !sameArray(localProver?.exportedFunctions, [
      "midnight_mobile_local_prover_check",
      "midnight_mobile_local_prover_close",
      "midnight_mobile_local_prover_configure",
      "midnight_mobile_local_prover_free",
      "midnight_mobile_local_prover_prove",
    ])
  ) {
    errors.push("Android local prover C ABI drifted");
  }
}

export function parseElfReport(header, dynamic, symbols) {
  const field = (name) =>
    new RegExp(`^\\s*${name}:\\s*(.+)$`, "mu").exec(header)?.[1]?.trim() ?? "";
  const needed = [...dynamic.matchAll(/Shared library: \[([^\]]+)\]/gu)]
    .map((match) => match[1])
    .sort();
  const functions = [
    ...symbols.matchAll(
      /\buniffi_midnight_mobile_runtime_fn_func_([a-z0-9_]+)$/gmu,
    ),
  ]
    .map((match) => match[1])
    .sort();
  const localProverFunctions = [
    ...symbols.matchAll(/\b(midnight_mobile_local_prover_[a-z0-9_]+)$/gmu),
  ]
    .map((match) => match[1])
    .sort();
  return {
    type: field("Type"),
    machine: field("Machine"),
    needed,
    functions,
    localProverFunctions,
  };
}

export function validateElfReport(report, target, config) {
  const errors = [];
  if (!report.type.startsWith("DYN ")) {
    errors.push(`${target.abi}: ELF type must be DYN`);
  }
  if (report.machine !== target.machine) {
    errors.push(`${target.abi}: ELF machine must be ${target.machine}`);
  }
  if (!sameArray(report.needed, [...config.android.neededLibraries].sort())) {
    errors.push(`${target.abi}: shared-library dependencies drifted`);
  }
  const functions = [...config.rust.uniffiFunctions].sort();
  if (!sameArray(report.functions, functions)) {
    errors.push(`${target.abi}: exported UniFFI function ABI drifted`);
  }
  if (
    !sameArray(
      report.localProverFunctions,
      [...config.android.localProver.exportedFunctions].sort(),
    )
  ) {
    errors.push(`${target.abi}: exported local prover C ABI drifted`);
  }
  return errors;
}

function loadConfiguration(repositoryRoot) {
  const config = JSON.parse(
    readFileSync(resolve(repositoryRoot, CONFIG_PATH), "utf8"),
  );
  const errors = validateNativeBuildConfig(config);
  if (errors.length > 0) throw new Error(errors.sort().join("\n"));
  return config;
}

function requireToolchain(repositoryRoot, config) {
  const rustc = runChecked(
    repositoryRoot,
    "rustc",
    ["--version"],
    "Rust version inspection",
  ).stdout.trim();
  if (!rustc.startsWith(`rustc ${config.rust.toolchain} `)) {
    throw new Error(
      `expected Rust ${config.rust.toolchain}; received ${rustc}`,
    );
  }
  const installed = runChecked(
    repositoryRoot,
    "rustup",
    ["target", "list", "--installed"],
    "Rust target inspection",
  ).stdout.split(/\s+/u);
  for (const target of config.android.targets) {
    if (!installed.includes(target.rustTarget)) {
      throw new Error(
        `required Rust target is not installed: ${target.rustTarget}`,
      );
    }
  }
}

function resolveNdk(repositoryRoot, config) {
  const sdkRoot = process.env.ANDROID_HOME ?? process.env.ANDROID_SDK_ROOT;
  if (sdkRoot === undefined || sdkRoot.length === 0) {
    throw new Error("ANDROID_HOME or ANDROID_SDK_ROOT is required");
  }
  const ndk = resolve(sdkRoot, "ndk", config.android.ndkVersion);
  if (!existsSync(resolve(ndk, "source.properties"))) {
    throw new Error(`required Android NDK is not installed: ${ndk}`);
  }
  const prebuiltRoot = resolve(ndk, "toolchains/llvm/prebuilt");
  const hostTags = readdirSync(prebuiltRoot).filter((entry) =>
    existsSync(resolve(prebuiltRoot, entry, "bin/llvm-readelf")),
  );
  if (hostTags.length !== 1) {
    throw new Error(
      "Android NDK must contain exactly one usable host toolchain",
    );
  }
  return resolve(prebuiltRoot, hostTags[0]);
}

function cargoEnvironment(toolchain, target, config) {
  const normalized = target.rustTarget.replaceAll("-", "_");
  const clang = resolve(
    toolchain,
    "bin",
    `${target.clangPrefix}${String(config.android.apiLevel)}-clang`,
  );
  if (!existsSync(clang))
    throw new Error(`required NDK linker is missing: ${clang}`);
  return {
    ...process.env,
    CARGO_INCREMENTAL: "0",
    SOURCE_DATE_EPOCH: "0",
    RUSTFLAGS:
      "-C debuginfo=0 -C strip=symbols -C link-arg=-Wl,--build-id=sha1",
    [`CARGO_TARGET_${normalized.toUpperCase()}_LINKER`]: clang,
    [`CC_${normalized}`]: clang,
    [`AR_${normalized}`]: resolve(toolchain, "bin/llvm-ar"),
  };
}

function cargoBuild(repositoryRoot, cargoTarget, target, config, toolchain) {
  const featureArguments = config.android.localProver.enabled
    ? ["--features", config.android.localProver.cargoFeatures.join(",")]
    : [];
  runChecked(
    repositoryRoot,
    "cargo",
    [
      "build",
      "--frozen",
      "--release",
      "--package",
      config.rust.package,
      "--target",
      target.rustTarget,
      "--target-dir",
      cargoTarget,
      ...featureArguments,
    ],
    `${target.abi} Rust release build`,
    { env: cargoEnvironment(toolchain, target, config), stdio: "inherit" },
  );
}

function cargoClean(repositoryRoot, cargoTarget, target, config) {
  runChecked(
    repositoryRoot,
    "cargo",
    [
      "clean",
      "--release",
      "--package",
      config.rust.package,
      "--target",
      target.rustTarget,
      "--target-dir",
      cargoTarget,
    ],
    `${target.abi} reproducibility clean`,
    { stdio: "inherit" },
  );
}

function inspectLibrary(repositoryRoot, path, target, config, toolchain) {
  const readelf = resolve(toolchain, "bin/llvm-readelf");
  const header = runChecked(
    repositoryRoot,
    readelf,
    ["-h", path],
    `${target.abi} ELF header inspection`,
  ).stdout;
  const dynamic = runChecked(
    repositoryRoot,
    readelf,
    ["-d", path],
    `${target.abi} ELF dependency inspection`,
  ).stdout;
  const symbols = runChecked(
    repositoryRoot,
    resolve(toolchain, "bin/llvm-nm"),
    ["-D", "--defined-only", path],
    `${target.abi} symbol inspection`,
  ).stdout;
  const errors = validateElfReport(
    parseElfReport(header, dynamic, symbols),
    target,
    config,
  );
  errors.push(...findForbiddenBinaryContent(path, readFileSync(path)));
  if (errors.length > 0) throw new Error(errors.sort().join("\n"));
}

function buildTarget(repositoryRoot, roots, target, config, toolchain) {
  cargoBuild(repositoryRoot, roots.cargo, target, config, toolchain);
  const source = resolve(
    roots.cargo,
    target.rustTarget,
    "release",
    `lib${config.rust.libraryBaseName}.so`,
  );
  const first = resolve(roots.repro, `${target.abi}-first.so`);
  copyFileSync(source, first);
  cargoClean(repositoryRoot, roots.cargo, target, config);
  cargoBuild(repositoryRoot, roots.cargo, target, config, toolchain);
  if (sha256(first) !== sha256(source)) {
    throw new Error(`${target.abi} native library is not reproducible`);
  }
  inspectLibrary(repositoryRoot, source, target, config, toolchain);
  const artifact = resolve(roots.output, target.abi, basename(source));
  const packaged = resolve(
    repositoryRoot,
    PACKAGE_ROOT,
    "android/src/main/jniLibs",
    target.abi,
    basename(source),
  );
  mkdirSync(resolve(artifact, ".."), { recursive: true });
  mkdirSync(resolve(packaged, ".."), { recursive: true });
  copyFileSync(source, artifact);
  copyFileSync(source, packaged);
  return {
    abi: target.abi,
    rustTarget: target.rustTarget,
    packagePath: `android/src/main/jniLibs/${target.abi}/${basename(source)}`,
    sha256: sha256(artifact),
    size: statSync(artifact).size,
  };
}

export function buildAndroidNativeDistribution(
  repositoryRoot = process.cwd(),
  artifactRoot = resolve(repositoryRoot, "artifacts/native/android"),
) {
  const config = loadConfiguration(repositoryRoot);
  requireToolchain(repositoryRoot, config);
  const toolchain = resolveNdk(repositoryRoot, config);
  const packaged = resolve(
    repositoryRoot,
    PACKAGE_ROOT,
    "android/src/main/jniLibs",
  );
  removeTree(artifactRoot);
  removeTree(packaged);
  const roots = {
    cargo: resolve(artifactRoot, "cargo-target"),
    output: resolve(artifactRoot, "lib"),
    repro: resolve(artifactRoot, "repro"),
  };
  mkdirSync(roots.repro, { recursive: true });
  const binaries = config.android.targets.map((target) =>
    buildTarget(repositoryRoot, roots, target, config, toolchain),
  );
  const totalBytes = binaries.reduce((total, binary) => total + binary.size, 0);
  if (totalBytes > config.android.combinedBudgetBytes) {
    throw new Error(
      `Android native payload exceeds 30 MiB: ${String(totalBytes)}`,
    );
  }
  const result = {
    schemaVersion: 1,
    ndkVersion: config.android.ndkVersion,
    apiLevel: config.android.apiLevel,
    cargoFeatures: config.android.localProver.cargoFeatures,
    shippingBudgetBytes: config.android.combinedBudgetBytes,
    totalBytes,
    binaries,
  };
  writeFileSync(
    resolve(artifactRoot, "android-binaries.json"),
    `${JSON.stringify(result, null, 2)}\n`,
  );
  return result;
}

function main() {
  try {
    const result = buildAndroidNativeDistribution();
    console.log(
      `Android native distribution passed: abis=${result.binaries.map((binary) => binary.abi).join(",")}, bytes=${String(result.totalBytes)}, reproducible=true`,
    );
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}

const invokedPath = process.argv[1];
if (
  invokedPath !== undefined &&
  pathToFileURL(resolve(invokedPath)).href === import.meta.url
) {
  main();
}
