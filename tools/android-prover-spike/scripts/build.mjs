import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const spikeRoot = resolve(scriptDirectory, "..");
const repositoryRoot = resolve(spikeRoot, "../..");
const targetRoot = resolve(repositoryRoot, "target/android-prover-spike");
const appInput = resolve(targetRoot, "app-input");
const rustTarget = "aarch64-linux-android";
const libraryName = "libmidnight_mobile_runtime.so";

function run(command, arguments_, options = {}) {
  const result = spawnSync(command, arguments_, {
    cwd: repositoryRoot,
    encoding: "utf8",
    maxBuffer: 30 * 1024 * 1024,
    ...options,
  });
  if (result.error !== undefined || result.status !== 0) {
    const detail = `${result.stderr ?? result.stdout ?? ""}`.trim();
    throw new Error(
      `${command} failed${detail.length === 0 ? "" : `:\n${detail}`}`,
    );
  }
  return result;
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function findNdkToolchain() {
  const sdk = process.env.ANDROID_HOME ?? process.env.ANDROID_SDK_ROOT;
  if (sdk === undefined || sdk.length === 0) {
    throw new Error("ANDROID_HOME or ANDROID_SDK_ROOT is required");
  }
  const prebuilt = resolve(sdk, "ndk/27.1.12297006/toolchains/llvm/prebuilt");
  const hosts = readdirSync(prebuilt).filter((host) =>
    existsSync(resolve(prebuilt, host, "bin/llvm-readelf")),
  );
  if (hosts.length !== 1) throw new Error("expected one NDK host toolchain");
  return resolve(prebuilt, hosts[0]);
}

function androidSdkRoot() {
  const sdk = process.env.ANDROID_HOME ?? process.env.ANDROID_SDK_ROOT;
  if (sdk === undefined || sdk.length === 0) {
    throw new Error("ANDROID_HOME or ANDROID_SDK_ROOT is required");
  }
  return sdk;
}

function cargoEnvironment(toolchain) {
  const clang = resolve(toolchain, "bin/aarch64-linux-android24-clang");
  return {
    ...process.env,
    AR_aarch64_linux_android: resolve(toolchain, "bin/llvm-ar"),
    CC_aarch64_linux_android: clang,
    CARGO_INCREMENTAL: "0",
    CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER: clang,
    RUSTFLAGS:
      "-C debuginfo=0 -C strip=symbols -C link-arg=-Wl,--build-id=sha1",
    SOURCE_DATE_EPOCH: "0",
  };
}

function inspectLibrary(library, toolchain) {
  const dynamic = run(resolve(toolchain, "bin/llvm-readelf"), [
    "-d",
    library,
  ]).stdout;
  const needed = [...dynamic.matchAll(/Shared library: \[([^\]]+)\]/gu)]
    .map((match) => match[1])
    .sort();
  const expected = ["libc.so", "libdl.so", "libm.so"];
  if (JSON.stringify(needed) !== JSON.stringify(expected)) {
    throw new Error(`unexpected native dependencies: ${needed.join(", ")}`);
  }
  const symbols = run(resolve(toolchain, "bin/llvm-nm"), [
    "-D",
    "--defined-only",
    library,
  ]).stdout;
  for (const symbol of [
    "midnight_mobile_local_prover_check",
    "midnight_mobile_local_prover_close",
    "midnight_mobile_local_prover_configure",
    "midnight_mobile_local_prover_free",
    "midnight_mobile_local_prover_prove",
  ]) {
    if (!symbols.includes(symbol))
      throw new Error(`missing native symbol ${symbol}`);
  }
  const uniffiFunctions = [
    ...symbols.matchAll(
      /\buniffi_midnight_mobile_runtime_fn_func_([a-z0-9_]+)$/gmu,
    ),
  ]
    .map((match) => match[1])
    .sort();
  const expectedUniFfiFunctions = [
    "apply_sync_batch",
    "begin_command",
    "cancel_operation",
    "close_wallet_session",
    "export_wallet_checkpoint",
    "get_wallet_snapshot",
    "open_wallet_session",
    "resume_operation",
  ];
  if (
    JSON.stringify(uniffiFunctions) !== JSON.stringify(expectedUniFfiFunctions)
  ) {
    throw new Error(`UniFFI ABI drifted: ${uniffiFunctions.join(", ")}`);
  }
  return needed;
}

function buildRust(toolchain) {
  const cargoTarget = resolve(targetRoot, "cargo-target");
  run(
    "cargo",
    [
      "build",
      "--offline",
      "--locked",
      "--release",
      "--package",
      "midnight-mobile-runtime",
      "--features",
      "local-prover",
      "--target",
      rustTarget,
      "--target-dir",
      cargoTarget,
    ],
    { env: cargoEnvironment(toolchain), stdio: "inherit" },
  );
  const library = resolve(cargoTarget, rustTarget, "release", libraryName);
  const destination = resolve(appInput, "jniLibs/arm64-v8a", libraryName);
  mkdirSync(dirname(destination), { recursive: true });
  copyFileSync(library, destination);
  return destination;
}

function gradleExecutable() {
  if (process.env.GRADLE !== undefined) return process.env.GRADLE;
  const command = spawnSync("gradle", ["--version"], { encoding: "utf8" });
  if (command.status === 0) return "gradle";
  throw new Error("set GRADLE to a compatible Gradle executable");
}

function buildApk() {
  rmSync(resolve(spikeRoot, "app/build"), { force: true, recursive: true });
  run(
    gradleExecutable(),
    ["--no-daemon", "--project-dir", spikeRoot, ":app:assembleRelease"],
    { stdio: "inherit" },
  );
  const source = resolve(
    spikeRoot,
    "app/build/outputs/apk/release/app-release.apk",
  );
  const destination = resolve(targetRoot, "android-prover-spike-release.apk");
  copyFileSync(source, destination);
  const listing = run("unzip", ["-lv", destination]).stdout;
  const assetPaths = [
    "assets/request.bin",
    "assets/check-request.bin",
    "assets/bls_midnight_2p15",
    "assets/zswap/9/spend.prover",
    "assets/zswap/9/spend.verifier",
    "assets/zswap/9/spend.bzkir",
  ];
  const storedAssets = [];
  for (const path of assetPaths) {
    const line = listing
      .split("\n")
      .find((candidate) => candidate.includes(path));
    if (line === undefined || !line.includes("Stored")) {
      throw new Error(`APK artifact is not stored uncompressed: ${path}`);
    }
    storedAssets.push({ path, compressionMethod: "Stored" });
  }
  const aapt2 = resolve(androidSdkRoot(), "build-tools/36.0.0/aapt2");
  const permissions = run(aapt2, ["dump", "permissions", destination]).stdout;
  if (/^uses-permission:/mu.test(permissions)) {
    throw new Error(`probe APK requests permissions:\n${permissions}`);
  }
  const badging = run(aapt2, ["dump", "badging", destination]).stdout;
  if (
    !badging.includes("minSdkVersion:'24'") ||
    !badging.includes("targetSdkVersion:'36'") ||
    !badging.includes("native-code: 'arm64-v8a'")
  ) {
    throw new Error("probe APK SDK or ABI metadata drifted");
  }
  return { path: destination, storedAssets, permissions: [] };
}

function sizeReport(apk, library, needed) {
  const manifest = JSON.parse(
    readFileSync(
      resolve(targetRoot, "artifacts/artifact-manifest.json"),
      "utf8",
    ),
  );
  const artifacts = manifest.artifacts.map((artifact) => ({
    path: artifact.path,
    sha256: artifact.sha256,
    size: artifact.size,
  }));
  const report = {
    schemaVersion: 1,
    rustTarget,
    apiLevel: 24,
    abi: "arm64-v8a",
    nativeLibrary: {
      path: basename(library),
      size: statSync(library).size,
      sha256: sha256(library),
      needed,
    },
    apk: {
      size: statSync(apk.path).size,
      sha256: sha256(apk.path),
      minSdk: 24,
      targetSdk: 36,
      abi: ["arm64-v8a"],
      requestedPermissions: apk.permissions,
      storedAssets: apk.storedAssets,
    },
    proofArtifacts: artifacts,
    proofArtifactBytes: artifacts.reduce(
      (sum, artifact) => sum + artifact.size,
      0,
    ),
  };
  writeFileSync(
    resolve(targetRoot, "android-build-report.json"),
    `${JSON.stringify(report, null, 2)}\n`,
  );
  return report;
}

function main() {
  if (!existsSync(resolve(targetRoot, "artifacts/artifact-manifest.json"))) {
    run("node", [resolve(scriptDirectory, "prepare-artifacts.mjs")], {
      stdio: "inherit",
    });
  }
  rmSync(appInput, { force: true, recursive: true });
  const toolchain = findNdkToolchain();
  const library = buildRust(toolchain);
  const needed = inspectLibrary(library, toolchain);
  const apk = buildApk();
  const report = sizeReport(apk, library, needed);
  console.log(
    `built ${apk.path}: native=${String(report.nativeLibrary.size)} ` +
      `apk=${String(report.apk.size)} artifacts=${String(report.proofArtifactBytes)}`,
  );
}

main();
