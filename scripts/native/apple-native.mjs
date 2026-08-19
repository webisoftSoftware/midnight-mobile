import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  chmodSync,
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  utimesSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";

import { removeTree } from "../quality/quality-utils.mjs";
import { findForbiddenBinaryContent } from "../wallet-core/check-wallet-core-artifacts.mjs";

const SCRIPT_DIRECTORY = dirname(fileURLToPath(import.meta.url));
export const REPOSITORY_ROOT = resolve(SCRIPT_DIRECTORY, "../..");
const CONFIGURATION_PATH = join(
  REPOSITORY_ROOT,
  "scripts/native/native-build-config.json",
);
export const NATIVE_CONFIGURATION = JSON.parse(
  readFileSync(CONFIGURATION_PATH, "utf8"),
);
export const APPLE_CONFIGURATION = NATIVE_CONFIGURATION.apple;
export const FRAMEWORK_NAME = APPLE_CONFIGURATION.frameworkName;
export const FRAMEWORK_BUNDLE = `${FRAMEWORK_NAME}.framework`;
export const XCFRAMEWORK_BUNDLE = `${FRAMEWORK_NAME}.xcframework`;
export const GENERATED_SWIFT_ROOT = join(
  REPOSITORY_ROOT,
  "packages/react-native/ios/generated",
);
export const LOCAL_PROVER_HEADER = "MidnightMobileLocalProverFFI.h";
const LOCAL_PROVER_HEADER_SOURCE = join(
  REPOSITORY_ROOT,
  "packages/react-native/ios",
  LOCAL_PROVER_HEADER,
);
export const SOURCE_DATE_EPOCH = 1_785_340_800;
export const XCODE_BUILD_CONCURRENCY_ARGUMENTS = Object.freeze(["-jobs", "2"]);

const ALLOWED_DEPENDENCIES = new Set([
  "/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation",
  "/System/Library/Frameworks/Security.framework/Security",
  "/usr/lib/libSystem.B.dylib",
  "/usr/lib/libiconv.2.dylib",
]);

export function fail(message) {
  throw new Error(`Apple native gate: ${message}`);
}

export function conciseFailureDetails(details) {
  const diagnosticLines = details
    .split("\n")
    .filter((line) =>
      /(?:^|\s)(?:error:|fatal error:|BUILD FAILED|Command .* failed)/iu.test(
        line,
      ),
    )
    .slice(-40);
  const tail = details.length > 12_000 ? details.slice(-12_000) : details;
  if (diagnosticLines.length === 0 || details.length <= 12_000) return tail;
  return `Extracted diagnostics:\n${diagnosticLines.join(
    "\n",
  )}\n\nOutput tail:\n${tail}`;
}

export function run(command, argumentsList, options = {}) {
  const result = spawnSync(command, argumentsList, {
    cwd: options.cwd ?? REPOSITORY_ROOT,
    encoding: "utf8",
    env: options.env ?? process.env,
    maxBuffer: 64 * 1024 * 1024,
    stdio: options.inherit === true ? "inherit" : "pipe",
  });
  if (result.error !== undefined) {
    fail(`unable to execute ${command}: ${result.error.message}`);
  }
  if (result.status !== 0) {
    const details = [result.stdout, result.stderr]
      .filter((value) => typeof value === "string" && value.length > 0)
      .join("\n")
      .trim();
    const concise = conciseFailureDetails(details);
    fail(
      `${command} exited with status ${String(result.status)}${
        concise.length === 0 ? "" : `:\n${concise}`
      }`,
    );
  }
  return `${result.stdout ?? ""}`;
}

export function collectFiles(root, path = root, output = []) {
  if (!existsSync(path)) return output;
  for (const entry of readdirSync(path).sort()) {
    const candidate = join(path, entry);
    const stat = statSync(candidate);
    if (stat.isDirectory()) collectFiles(root, candidate, output);
    else if (stat.isFile()) {
      output.push(relative(root, candidate).replaceAll("\\", "/"));
    } else {
      fail(`unsupported filesystem entry: ${candidate}`);
    }
  }
  return output;
}

export function fingerprintTree(path) {
  const hash = createHash("sha256");
  const files = collectFiles(path);
  for (const file of files) {
    hash.update(file);
    hash.update("\0");
    hash.update(readFileSync(join(path, file)));
    hash.update("\0");
  }
  return { files, sha256: hash.digest("hex") };
}

function frameworkPlist(platform) {
  const supportedPlatform =
    platform === "device" ? "iPhoneOS" : "iPhoneSimulator";
  return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleExecutable</key>
  <string>${FRAMEWORK_NAME}</string>
  <key>CFBundleIdentifier</key>
  <string>dev.oneam.${FRAMEWORK_NAME}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>${FRAMEWORK_NAME}</string>
  <key>CFBundlePackageType</key>
  <string>FMWK</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleSupportedPlatforms</key>
  <array><string>${supportedPlatform}</string></array>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>MinimumOSVersion</key>
  <string>${APPLE_CONFIGURATION.deploymentTarget}</string>
</dict>
</plist>
`;
}

export function canonicalXcframeworkPlist() {
  return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>AvailableLibraries</key>
  <array>
    <dict>
      <key>BinaryPath</key>
      <string>${FRAMEWORK_BUNDLE}/${FRAMEWORK_NAME}</string>
      <key>LibraryIdentifier</key>
      <string>ios-arm64</string>
      <key>LibraryPath</key>
      <string>${FRAMEWORK_BUNDLE}</string>
      <key>SupportedArchitectures</key>
      <array><string>arm64</string></array>
      <key>SupportedPlatform</key>
      <string>ios</string>
    </dict>
    <dict>
      <key>BinaryPath</key>
      <string>${FRAMEWORK_BUNDLE}/${FRAMEWORK_NAME}</string>
      <key>LibraryIdentifier</key>
      <string>ios-arm64_x86_64-simulator</string>
      <key>LibraryPath</key>
      <string>${FRAMEWORK_BUNDLE}</string>
      <key>SupportedArchitectures</key>
      <array><string>arm64</string><string>x86_64</string></array>
      <key>SupportedPlatform</key>
      <string>ios</string>
      <key>SupportedPlatformVariant</key>
      <string>simulator</string>
    </dict>
  </array>
  <key>CFBundlePackageType</key>
  <string>XFWK</string>
  <key>XCFrameworkFormatVersion</key>
  <string>1.0</string>
</dict>
</plist>
`;
}

export function createFramework(destination, binary, platform) {
  removeTree(destination);
  mkdirSync(join(destination, "Headers"), { recursive: true });
  mkdirSync(join(destination, "Modules"), { recursive: true });
  cpSync(binary, join(destination, FRAMEWORK_NAME));
  chmodSync(join(destination, FRAMEWORK_NAME), 0o755);
  cpSync(
    join(GENERATED_SWIFT_ROOT, `${FRAMEWORK_NAME}FFI.h`),
    join(destination, "Headers", `${FRAMEWORK_NAME}FFI.h`),
  );
  cpSync(
    LOCAL_PROVER_HEADER_SOURCE,
    join(destination, "Headers", LOCAL_PROVER_HEADER),
  );
  const modulemap = readFileSync(
    join(GENERATED_SWIFT_ROOT, `${FRAMEWORK_NAME}FFI.modulemap`),
    "utf8",
  );
  writeFileSync(
    join(destination, "Modules/module.modulemap"),
    modulemap
      .replace(/^module \S+/u, `framework module ${FRAMEWORK_NAME}`)
      .replace(
        `header "${FRAMEWORK_NAME}FFI.h"`,
        `header "${FRAMEWORK_NAME}FFI.h"\n    header "${LOCAL_PROVER_HEADER}"`,
      ),
  );
  writeFileSync(join(destination, "Info.plist"), frameworkPlist(platform));
}

function parseBuildVersion(loadCommands) {
  const match =
    /cmd LC_BUILD_VERSION[\s\S]*?platform\s+([0-9]+)[\s\S]*?minos\s+([0-9.]+)/u.exec(
      loadCommands,
    );
  if (match === null) fail("binary has no LC_BUILD_VERSION");
  return { platform: Number(match[1]), minimumVersion: match[2] };
}

function validateArchitectures(binary, expected, errors) {
  const actual = run("xcrun", ["lipo", "-archs", binary])
    .trim()
    .split(/\s+/u)
    .filter(Boolean)
    .sort();
  if (!isDeepStrictEqual(actual, [...expected].sort())) {
    errors.push(
      `${binary}: architectures ${actual.join(",")} do not equal ${expected.join(",")}`,
    );
  }
}

function validateMachO(binary, expectedPlatform, errors) {
  const loadCommands = run("xcrun", ["otool", "-l", binary]);
  if (!loadCommands.includes("cmd LC_ID_DYLIB")) {
    errors.push(`${binary}: Mach-O file type is not a dynamic library`);
  }
  const build = parseBuildVersion(loadCommands);
  const platformNumber = expectedPlatform === "device" ? 2 : 7;
  if (build.platform !== platformNumber) {
    errors.push(`${binary}: platform is ${String(build.platform)}`);
  }
  if (build.minimumVersion !== APPLE_CONFIGURATION.deploymentTarget) {
    errors.push(`${binary}: minimum iOS version is ${build.minimumVersion}`);
  }
  const installNames = run("xcrun", ["otool", "-D", binary])
    .trim()
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.startsWith("@rpath/"));
  if (
    installNames.length === 0 ||
    installNames.some((name) => name !== APPLE_CONFIGURATION.installName)
  ) {
    errors.push(`${binary}: dynamic install name is incorrect`);
  }
}

function validateDependencies(binary, errors) {
  const dependencies = run("xcrun", ["otool", "-L", binary])
    .trim()
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.includes(" (compatibility version"))
    .map((line) => line.split(" (compatibility version")[0])
    .filter((dependency) => dependency !== APPLE_CONFIGURATION.installName);
  for (const dependency of dependencies) {
    if (!ALLOWED_DEPENDENCIES.has(dependency)) {
      errors.push(`${binary}: unsupported dynamic dependency ${dependency}`);
    }
  }
}

function validateExports(binary, errors) {
  const prefix = "_uniffi_midnight_mobile_runtime_fn_func_";
  const exports = run("xcrun", ["nm", "-gU", binary])
    .split("\n")
    .map((line) => line.trim().split(/\s+/u).at(-1) ?? "")
    .filter((symbol) => symbol.startsWith(prefix))
    .map((symbol) => symbol.slice(prefix.length))
    .sort();
  const expected = [...NATIVE_CONFIGURATION.rust.uniffiFunctions].sort();
  if (!isDeepStrictEqual(exports, expected)) {
    errors.push(`${binary}: UniFFI exports are ${exports.join(",")}`);
  }
  const localProverExports = run("xcrun", ["nm", "-gU", binary])
    .split("\n")
    .map((line) => line.trim().split(/\s+/u).at(-1) ?? "")
    .filter((symbol) => symbol.startsWith("_midnight_mobile_local_prover_"))
    .map((symbol) => symbol.slice(1))
    .sort();
  const expectedLocalProver = [
    ...APPLE_CONFIGURATION.localProver.exportedFunctions,
  ].sort();
  if (!isDeepStrictEqual(localProverExports, expectedLocalProver)) {
    errors.push(
      `${binary}: local prover exports are ${localProverExports.join(",")}`,
    );
  }
}

function validateBinary(binary, architectures, platform, errors) {
  validateArchitectures(binary, architectures, errors);
  validateMachO(binary, platform, errors);
  validateDependencies(binary, errors);
  validateExports(binary, errors);
  errors.push(...findForbiddenBinaryContent(binary, readFileSync(binary)));
}

function parsePlist(path) {
  return JSON.parse(run("plutil", ["-convert", "json", "-o", "-", path]));
}

function validateFramework(framework, library, errors) {
  const binary = join(framework, FRAMEWORK_NAME);
  const platform =
    library.SupportedPlatformVariant === "simulator" ? "simulator" : "device";
  const expectedArchitectures =
    platform === "simulator" ? ["arm64", "x86_64"] : ["arm64"];
  if (
    library.SupportedPlatform !== "ios" ||
    !isDeepStrictEqual(
      [...library.SupportedArchitectures].sort(),
      expectedArchitectures.sort(),
    )
  ) {
    errors.push(`${framework}: XCFramework slice metadata is incorrect`);
  }
  const plist = parsePlist(join(framework, "Info.plist"));
  if (plist.MinimumOSVersion !== APPLE_CONFIGURATION.deploymentTarget) {
    errors.push(`${framework}: framework MinimumOSVersion is incorrect`);
  }
  const modulemap = readFileSync(
    join(framework, "Modules/module.modulemap"),
    "utf8",
  );
  if (
    !modulemap.includes(`framework module ${FRAMEWORK_NAME}`) ||
    !modulemap.includes(`header "${FRAMEWORK_NAME}FFI.h"`) ||
    !modulemap.includes(`header "${LOCAL_PROVER_HEADER}"`)
  ) {
    errors.push(`${framework}: FFI module map is incorrect`);
  }
  validateBinary(binary, expectedArchitectures, platform, errors);
}

export function validateXcframework(xcframework) {
  const errors = [];
  const metadata = parsePlist(join(xcframework, "Info.plist"));
  const libraries = metadata.AvailableLibraries;
  if (!Array.isArray(libraries) || libraries.length !== 2) {
    fail("XCFramework must contain exactly device and simulator libraries");
  }
  const expectedFiles = ["Info.plist"];
  for (const library of libraries) {
    const expectedIdentifier =
      library.SupportedPlatformVariant === "simulator"
        ? "ios-arm64_x86_64-simulator"
        : "ios-arm64";
    if (
      library.LibraryIdentifier !== expectedIdentifier ||
      library.LibraryPath !== FRAMEWORK_BUNDLE
    ) {
      errors.push("XCFramework slice path metadata is incorrect");
      continue;
    }
    const prefix = `${library.LibraryIdentifier}/${FRAMEWORK_BUNDLE}`;
    expectedFiles.push(
      `${prefix}/Headers/${FRAMEWORK_NAME}FFI.h`,
      `${prefix}/Headers/${LOCAL_PROVER_HEADER}`,
      `${prefix}/Info.plist`,
      `${prefix}/Modules/module.modulemap`,
      `${prefix}/${FRAMEWORK_NAME}`,
    );
    const framework = join(
      xcframework,
      library.LibraryIdentifier,
      library.LibraryPath,
    );
    if (!existsSync(framework)) {
      errors.push(`${framework}: declared XCFramework slice is missing`);
      continue;
    }
    validateFramework(framework, library, errors);
  }
  const actualFiles = collectFiles(xcframework);
  if (!isDeepStrictEqual(actualFiles, expectedFiles.sort())) {
    errors.push(`XCFramework file set is ${actualFiles.join(",")}`);
  }
  if (errors.length > 0) fail(errors.sort().join("\n"));
  return fingerprintTree(xcframework);
}

export function canonicalizeXcframeworkMetadata(xcframework) {
  validateXcframework(xcframework);
  writeFileSync(join(xcframework, "Info.plist"), canonicalXcframeworkPlist());
  return validateXcframework(xcframework);
}

function normalizeTimestamps(path) {
  const date = new Date(SOURCE_DATE_EPOCH * 1_000);
  for (const file of collectFiles(path)) {
    utimesSync(join(path, file), date, date);
  }
  const directories = [];
  const visit = (directory) => {
    directories.push(directory);
    for (const entry of readdirSync(directory)) {
      const candidate = join(directory, entry);
      if (statSync(candidate).isDirectory()) visit(candidate);
    }
  };
  visit(path);
  for (const directory of directories.reverse()) {
    utimesSync(directory, date, date);
  }
}

export function createCompressedArchive(xcframework, archive, stagingRoot) {
  mkdirSync(stagingRoot, { recursive: true });
  const staged = join(stagingRoot, XCFRAMEWORK_BUNDLE);
  removeTree(staged);
  cpSync(xcframework, staged, { recursive: true });
  normalizeTimestamps(staged);
  mkdirSync(dirname(archive), { recursive: true });
  rmSync(archive, { force: true });
  run("/usr/bin/zip", ["-X", "-q", "-r", archive, XCFRAMEWORK_BUNDLE], {
    cwd: stagingRoot,
  });
  const size = statSync(archive).size;
  if (size > APPLE_CONFIGURATION.compressedBudgetBytes) {
    fail(
      `compressed XCFramework is ${String(size)} bytes; budget is ${String(
        APPLE_CONFIGURATION.compressedBudgetBytes,
      )}`,
    );
  }
  return size;
}
