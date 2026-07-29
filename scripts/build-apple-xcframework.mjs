import { cpSync, existsSync, mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";

import {
  APPLE_CONFIGURATION,
  canonicalizeXcframeworkMetadata,
  createCompressedArchive,
  createFramework,
  fail,
  fingerprintTree,
  FRAMEWORK_BUNDLE,
  FRAMEWORK_NAME,
  NATIVE_CONFIGURATION,
  REPOSITORY_ROOT,
  run,
  SOURCE_DATE_EPOCH,
  validateXcframework,
  XCFRAMEWORK_BUNDLE,
} from "./apple-native.mjs";

const DEFAULT_OUTPUT = join(
  REPOSITORY_ROOT,
  "packages/react-native/ios/build",
  XCFRAMEWORK_BUNDLE,
);
const DEFAULT_ARCHIVE = join(
  REPOSITORY_ROOT,
  "artifacts/apple",
  `${XCFRAMEWORK_BUNDLE}.zip`,
);

function option(argumentsList, name, fallback) {
  const index = argumentsList.indexOf(name);
  if (index < 0) return fallback;
  const value = argumentsList[index + 1];
  if (value === undefined || value.startsWith("--")) {
    fail(`${name} requires a path`);
  }
  return resolve(value);
}

function parseArguments(argumentsList) {
  const known = new Set(["--output", "--archive", "--single-pass"]);
  for (let index = 0; index < argumentsList.length; index += 1) {
    const argument = argumentsList[index];
    if (!known.has(argument)) fail(`unsupported argument: ${argument}`);
    if (argument !== "--single-pass") index += 1;
  }
  return {
    archive: option(argumentsList, "--archive", DEFAULT_ARCHIVE),
    output: option(argumentsList, "--output", DEFAULT_OUTPUT),
    verifyReproducible: !argumentsList.includes("--single-pass"),
  };
}

function validateOutputPath(path) {
  if (!path.endsWith(`/${XCFRAMEWORK_BUNDLE}`)) {
    fail(`output must end in ${XCFRAMEWORK_BUNDLE}`);
  }
  if (path === REPOSITORY_ROOT || path === dirname(REPOSITORY_ROOT)) {
    fail("refusing broad output path");
  }
}

function verifyToolchain() {
  const release = /^rustc ([0-9.]+)/u.exec(run("rustc", ["--version"]))?.[1];
  if (release !== NATIVE_CONFIGURATION.rust.toolchain) {
    fail(
      `rustc ${String(release)} does not match ${NATIVE_CONFIGURATION.rust.toolchain}`,
    );
  }
  const installed = new Set(
    run("rustup", ["target", "list", "--installed"]).trim().split("\n"),
  );
  for (const target of APPLE_CONFIGURATION.targets) {
    if (!installed.has(target.rustTarget)) {
      fail(`required Rust target is not installed: ${target.rustTarget}`);
    }
    run("xcrun", ["--sdk", target.sdk, "--show-sdk-path"]);
  }
}

function rustFlags() {
  return [
    `--remap-path-prefix=${REPOSITORY_ROOT}=/midnight-mobile`,
    "-C",
    "link-arg=-install_name",
    "-C",
    `link-arg=${APPLE_CONFIGURATION.installName}`,
    "-C",
    "link-arg=-dead_strip",
  ].join("\u001f");
}

function buildTarget(root, target) {
  const targetDirectory = join(root, "cargo-target");
  const environment = {
    ...process.env,
    CARGO_ENCODED_RUSTFLAGS: rustFlags(),
    CARGO_INCREMENTAL: "0",
    CARGO_TARGET_DIR: targetDirectory,
    IPHONEOS_DEPLOYMENT_TARGET: APPLE_CONFIGURATION.deploymentTarget,
    SOURCE_DATE_EPOCH: String(SOURCE_DATE_EPOCH),
    ZERO_AR_DATE: "1",
  };
  run(
    "cargo",
    [
      "build",
      "--locked",
      "--offline",
      "--release",
      "--package",
      NATIVE_CONFIGURATION.rust.package,
      "--target",
      target.rustTarget,
    ],
    { env: environment, inherit: true },
  );
  const library = join(
    targetDirectory,
    target.rustTarget,
    "release",
    `lib${NATIVE_CONFIGURATION.rust.libraryBaseName}.dylib`,
  );
  if (!existsSync(library)) fail(`Cargo did not produce ${library}`);
  return library;
}

function assembleXcframework(root) {
  const libraries = new Map();
  for (const target of APPLE_CONFIGURATION.targets) {
    libraries.set(target.rustTarget, buildTarget(root, target));
  }
  const frameworks = join(root, "frameworks");
  const deviceFramework = join(frameworks, "device", FRAMEWORK_BUNDLE);
  const simulatorFramework = join(frameworks, "simulator", FRAMEWORK_BUNDLE);
  createFramework(
    deviceFramework,
    libraries.get("aarch64-apple-ios"),
    "device",
  );
  const universalSimulator = join(root, FRAMEWORK_NAME);
  run("xcrun", [
    "lipo",
    "-create",
    libraries.get("aarch64-apple-ios-sim"),
    libraries.get("x86_64-apple-ios"),
    "-output",
    universalSimulator,
  ]);
  createFramework(simulatorFramework, universalSimulator, "simulator");
  const output = join(root, XCFRAMEWORK_BUNDLE);
  run("xcodebuild", [
    "-create-xcframework",
    "-framework",
    deviceFramework,
    "-framework",
    simulatorFramework,
    "-output",
    output,
  ]);
  canonicalizeXcframeworkMetadata(output);
  return output;
}

function compareBuilds(first, second) {
  const firstFingerprint = fingerprintTree(first);
  const secondFingerprint = fingerprintTree(second);
  if (
    firstFingerprint.sha256 !== secondFingerprint.sha256 ||
    JSON.stringify(firstFingerprint.files) !==
      JSON.stringify(secondFingerprint.files)
  ) {
    fail(
      `XCFramework is not reproducible: ${firstFingerprint.sha256} != ${secondFingerprint.sha256}`,
    );
  }
  return firstFingerprint;
}

export function buildAppleXcframework(argumentsList = process.argv.slice(2)) {
  const options = parseArguments(argumentsList);
  validateOutputPath(options.output);
  verifyToolchain();
  const temporaryRoot = mkdtempSync(join(tmpdir(), "midnight-apple-build-"));
  try {
    const first = assembleXcframework(join(temporaryRoot, "first"));
    const fingerprint = options.verifyReproducible
      ? compareBuilds(first, assembleXcframework(join(temporaryRoot, "second")))
      : fingerprintTree(first);
    rmSync(options.output, { force: true, recursive: true });
    mkdirSync(dirname(options.output), { recursive: true });
    cpSync(first, options.output, { recursive: true });
    validateXcframework(options.output);
    const archiveSize = createCompressedArchive(
      options.output,
      options.archive,
      join(temporaryRoot, "archive"),
    );
    console.log(
      `Apple XCFramework passed: sha256=${fingerprint.sha256}, archive-bytes=${String(
        archiveSize,
      )}, reproducible=${String(options.verifyReproducible)}`,
    );
  } finally {
    rmSync(temporaryRoot, { force: true, recursive: true });
  }
}

buildAppleXcframework();
