import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  statSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, relative, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { isDeepStrictEqual } from "node:util";
import { removeTree } from "./quality-utils.mjs";

const PACKAGE_ROOT = "packages/react-native";
const EXPECTED_FILES = Object.freeze([
  "README.md",
  "dist",
  "expo-module.config.json",
  "ios/MidnightMobileRuntime.podspec",
  "ios/MidnightMobileLocalProverModule.swift",
  "ios/MidnightMobileLocalProverArtifacts.swift",
  "ios/MidnightMobileRuntimeModule.swift",
  "ios/MidnightMobileLocalProverFFI.h",
  "ios/build/MidnightMobileRuntime.xcframework",
  "ios/generated",
  "android/build.gradle",
  "android/consumer-rules.pro",
  "android/local-prover",
  "android/src/main",
  "android/generated",
]);
const EXPECTED_PEERS = Object.freeze({
  expo: ">=55.0.0 <56.0.0",
  "expo-modules-core": ">=55.0.0 <56.0.0",
  react: ">=19.2.0 <20.0.0",
  "react-native": ">=0.83.0 <0.84.0",
});
const EXPECTED_TESTED_DEPENDENCIES = Object.freeze({
  "@types/node": "24.10.1",
  "@types/react": "19.2.14",
  expo: "55.0.28",
  "expo-modules-core": "55.0.25",
  react: "19.2.0",
  "react-native": "0.83.6",
  typescript: "5.9.3",
});
const EXPECTED_ENGINES = Object.freeze({ node: ">=22 <23 || >=24 <25" });
const REQUIRED_PACKED_FILES = Object.freeze([
  "README.md",
  "android/build.gradle",
  "android/consumer-rules.pro",
  "android/generated/README.md",
  "android/local-prover/dev/oneam/midnightmobile/localprover/LocalProverBridge.kt",
  "android/src/main/jniLibs/arm64-v8a/libmidnight_mobile_runtime.so",
  "android/src/main/jniLibs/x86_64/libmidnight_mobile_runtime.so",
  "android/src/main/java/dev/oneam/midnightmobile/MidnightMobileRuntimeModule.kt",
  "android/src/main/java/dev/oneam/midnightmobile/localprover/MidnightMobileLocalProverModule.kt",
  "dist/index.d.ts",
  "dist/index.js",
  "dist/local-prover.d.ts",
  "dist/local-prover.js",
  "expo-module.config.json",
  "ios/MidnightMobileRuntime.podspec",
  "ios/MidnightMobileLocalProverModule.swift",
  "ios/MidnightMobileLocalProverArtifacts.swift",
  "ios/MidnightMobileRuntimeModule.swift",
  "ios/MidnightMobileLocalProverFFI.h",
  "ios/build/MidnightMobileRuntime.xcframework/Info.plist",
  "ios/generated/README.md",
  "package.json",
]);
const ALLOWED_PACKED_PATH =
  /^(?:README\.md|package\.json|dist\/|expo-module\.config\.json$|android\/(?:build\.gradle$|consumer-rules\.pro$|generated\/|local-prover\/|src\/main\/)|ios\/(?:MidnightMobileRuntime\.podspec$|MidnightMobile(?:LocalProver(?:Module|Artifacts)|RuntimeModule)\.swift$|MidnightMobileLocalProverFFI\.h$|build\/MidnightMobileRuntime\.xcframework\/|generated\/))/u;
const ALLOWED_NATIVE_BINARY_PATH =
  /^(?:android\/src\/main\/jniLibs\/(?:arm64-v8a|x86_64)\/libmidnight_mobile_runtime\.so|ios\/build\/MidnightMobileRuntime\.xcframework\/[^/]+\/MidnightMobileRuntime\.framework\/MidnightMobileRuntime)$/u;
const NATIVE_BINARY_PATH = /\.(?:a|aar|dll|dylib|so)(?:\/|$)/u;

function isObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function commandError(label, result) {
  const detail = `${result.stderr ?? result.stdout ?? ""}`.trim();
  return `${label} failed${detail.length === 0 ? "" : `:\n${detail}`}`;
}

function run(repositoryRoot, command, arguments_, cwd = repositoryRoot) {
  return spawnSync(command, arguments_, {
    cwd,
    encoding: "utf8",
    maxBuffer: 20 * 1024 * 1024,
  });
}

function runChecked(repositoryRoot, command, arguments_, label, cwd) {
  const result = run(repositoryRoot, command, arguments_, cwd);
  if (result.status !== 0) throw new Error(commandError(label, result));
  return result;
}

function collectFiles(root, path = root, output = []) {
  if (!existsSync(path)) return output;
  for (const entry of readdirSync(path)) {
    const candidate = resolve(path, entry);
    if (statSync(candidate).isDirectory()) {
      collectFiles(root, candidate, output);
    } else {
      output.push(relative(root, candidate).replaceAll("\\", "/"));
    }
  }
  return output.sort();
}

function fingerprintDirectory(path) {
  const hash = createHash("sha256");
  const files = collectFiles(path);
  for (const file of files) {
    hash.update(file);
    hash.update("\0");
    hash.update(readFileSync(resolve(path, file)));
    hash.update("\0");
  }
  return { files, hash: hash.digest("hex") };
}

function validateExact(actual, expected, label, errors) {
  if (!isDeepStrictEqual(actual, expected)) {
    errors.push(`${label} must equal ${JSON.stringify(expected)}`);
  }
}

export function validatePackageMetadata(metadata, autolinking) {
  const errors = [];
  if (!isObject(metadata)) return ["package metadata must be an object"];
  validateExact(metadata.name, "@1am/midnight-mobile", "name", errors);
  validateExact(metadata.version, "0.1.0-alpha.1", "version", errors);
  validateExact(metadata.type, "module", "type", errors);
  validateExact(metadata.main, "./dist/index.js", "main", errors);
  validateExact(metadata.types, "./dist/index.d.ts", "types", errors);
  validateExact(
    metadata.exports,
    {
      ".": {
        types: "./dist/index.d.ts",
        import: "./dist/index.js",
        default: "./dist/index.js",
      },
      "./local-prover": {
        types: "./dist/local-prover.d.ts",
        import: "./dist/local-prover.js",
        default: "./dist/local-prover.js",
      },
      "./package.json": "./package.json",
    },
    "exports",
    errors,
  );
  validateExact(metadata.files, EXPECTED_FILES, "files", errors);
  validateExact(
    metadata.peerDependencies,
    EXPECTED_PEERS,
    "peerDependencies",
    errors,
  );
  validateExact(
    metadata.devDependencies,
    EXPECTED_TESTED_DEPENDENCIES,
    "devDependencies",
    errors,
  );
  validateExact(metadata.sideEffects, false, "sideEffects", errors);
  validateExact(metadata.engines, EXPECTED_ENGINES, "engines", errors);
  for (const field of [
    "dependencies",
    "optionalDependencies",
    "bundledDependencies",
  ]) {
    if (metadata[field] !== undefined) {
      errors.push(`${field} must be absent`);
    }
  }
  if (metadata.scripts?.postinstall !== undefined) {
    errors.push("scripts.postinstall must be absent");
  }
  validateExact(
    autolinking,
    {
      platforms: ["apple", "android"],
      apple: {
        swiftModuleName: "MidnightMobileExpo",
        modules: [
          "MidnightMobileRuntimeModule",
          "MidnightMobileLocalProverModule",
        ],
      },
      android: {
        modules: [
          "dev.oneam.midnightmobile.MidnightMobileRuntimeModule",
          "dev.oneam.midnightmobile.localprover.MidnightMobileLocalProverModule",
        ],
      },
    },
    "Expo autolinking metadata",
    errors,
  );
  return errors;
}

export function validatePackEntries(entries) {
  const errors = [];
  const paths = entries
    .filter((entry) => typeof entry?.path === "string")
    .map((entry) => entry.path)
    .sort();
  for (const required of REQUIRED_PACKED_FILES) {
    if (!paths.includes(required)) {
      errors.push(`npm tarball is missing ${required}`);
    }
  }
  for (const path of paths) {
    if (!ALLOWED_PACKED_PATH.test(path)) {
      errors.push(`npm tarball contains non-allowlisted path: ${path}`);
    }
    if (
      NATIVE_BINARY_PATH.test(path) &&
      !ALLOWED_NATIVE_BINARY_PATH.test(path)
    ) {
      errors.push(`npm tarball contains unsupported native binary: ${path}`);
    }
  }
  return errors;
}

function buildReproducibly(repositoryRoot) {
  const dist = resolve(repositoryRoot, PACKAGE_ROOT, "dist");
  const fingerprints = [];
  for (let attempt = 0; attempt < 2; attempt += 1) {
    removeTree(dist);
    runChecked(
      repositoryRoot,
      "npm",
      ["run", "build", "--workspace", "@1am/midnight-mobile"],
      `package build ${String(attempt + 1)}`,
    );
    fingerprints.push(fingerprintDirectory(dist));
  }
  if (!isDeepStrictEqual(fingerprints[0], fingerprints[1])) {
    throw new Error("package build output is not byte-for-byte reproducible");
  }
  return fingerprints[0].files.length;
}

function packPackage(repositoryRoot, temporaryRoot) {
  const result = runChecked(
    repositoryRoot,
    "npm",
    [
      "pack",
      "--json",
      "--pack-destination",
      temporaryRoot,
      "--cache",
      resolve(temporaryRoot, "npm-cache"),
      resolve(repositoryRoot, PACKAGE_ROOT),
    ],
    "npm package assembly",
  );
  let report;
  try {
    report = JSON.parse(result.stdout)[0];
  } catch {
    throw new Error("npm package assembly returned invalid JSON");
  }
  if (!isObject(report) || typeof report.filename !== "string") {
    throw new Error("npm package assembly did not report a tarball");
  }
  const errors = validatePackEntries(
    Array.isArray(report.files) ? report.files : [],
  );
  if (errors.length > 0) throw new Error(errors.sort().join("\n"));
  return {
    entries: report.files.length,
    tarball: resolve(temporaryRoot, report.filename),
  };
}

function consumerSource() {
  return `import {
  InMemoryMidnightCheckpointStore,
  MidnightRuntimeController,
  MidnightRuntimeProvider,
  createMidnightRuntimeApi,
  createStandardMidnightTransport,
  useMidnightRuntime,
  type MidnightCheckpoint,
  type MidnightCommand,
  type MidnightCommandResult,
  type MidnightFetch,
  type MidnightLogger,
  type MidnightNetworkConfiguration,
  type MidnightRuntimeApi,
  type MidnightRuntimeContextValue,
  type MidnightRuntimeErrorCode,
  type MidnightRuntimeProviderProps,
  type MidnightSessionHandle,
  type MidnightWalletSnapshot,
} from "@1am/midnight-mobile";

export const publicValues = [
  InMemoryMidnightCheckpointStore,
  MidnightRuntimeController,
  MidnightRuntimeProvider,
  createMidnightRuntimeApi,
  createStandardMidnightTransport,
  useMidnightRuntime,
];
export type PublicTypes = readonly [
  MidnightCheckpoint,
  MidnightCommand<"transfer">,
  MidnightCommandResult<"transfer">,
  MidnightFetch,
  MidnightLogger,
  MidnightNetworkConfiguration,
  MidnightRuntimeApi,
  MidnightRuntimeContextValue,
  MidnightRuntimeErrorCode,
  MidnightRuntimeProviderProps,
  MidnightSessionHandle,
  MidnightWalletSnapshot,
];
`;
}

function writeConsumerFiles(consumer, tarball) {
  mkdirSync(consumer, { recursive: true });
  writeFileSync(
    resolve(consumer, "package.json"),
    `${JSON.stringify(
      {
        name: "midnight-mobile-package-consumer",
        version: "0.0.0",
        private: true,
        type: "module",
        dependencies: {
          "@1am/midnight-mobile": pathToFileURL(tarball).href,
        },
      },
      null,
      2,
    )}\n`,
  );
  writeFileSync(resolve(consumer, "consumer.ts"), consumerSource());
  writeFileSync(
    resolve(consumer, "tsconfig.json"),
    `${JSON.stringify(
      {
        compilerOptions: {
          exactOptionalPropertyTypes: true,
          lib: ["ES2022", "DOM"],
          module: "NodeNext",
          moduleResolution: "NodeNext",
          noEmit: true,
          noUncheckedIndexedAccess: true,
          skipLibCheck: true,
          strict: true,
          target: "ES2022",
        },
        include: ["consumer.ts"],
      },
      null,
      2,
    )}\n`,
  );
}

function linkRootLockedPeers(repositoryRoot, consumer) {
  for (const dependency of [
    "expo",
    "expo-modules-core",
    "react",
    "react-native",
  ]) {
    const installed = JSON.parse(
      readFileSync(
        resolve(repositoryRoot, "node_modules", dependency, "package.json"),
        "utf8",
      ),
    );
    if (installed.version !== EXPECTED_TESTED_DEPENDENCIES[dependency]) {
      throw new Error(`root-locked ${dependency} version does not match M3`);
    }
    symlinkSync(
      resolve(repositoryRoot, "node_modules", dependency),
      resolve(consumer, "node_modules", dependency),
      "junction",
    );
  }
  mkdirSync(resolve(consumer, "node_modules/@types"), { recursive: true });
  symlinkSync(
    resolve(repositoryRoot, "node_modules/@types/react"),
    resolve(consumer, "node_modules/@types/react"),
    "junction",
  );
}

function validateConsumerInstall(repositoryRoot, temporaryRoot, tarball) {
  const consumer = resolve(temporaryRoot, "consumer");
  writeConsumerFiles(consumer, tarball);
  runChecked(
    repositoryRoot,
    "npm",
    [
      "install",
      "--ignore-scripts",
      "--legacy-peer-deps",
      "--offline",
      "--no-audit",
      "--no-fund",
      "--cache",
      resolve(temporaryRoot, "npm-cache"),
    ],
    "isolated offline consumer install",
    consumer,
  );
  linkRootLockedPeers(repositoryRoot, consumer);
  runChecked(
    repositoryRoot,
    process.execPath,
    [
      resolve(repositoryRoot, "node_modules/typescript/bin/tsc"),
      "--project",
      "tsconfig.json",
    ],
    "installed declaration consumer typecheck",
    consumer,
  );
}

export function runPackageCheck(repositoryRoot = process.cwd()) {
  const metadata = JSON.parse(
    readFileSync(resolve(repositoryRoot, PACKAGE_ROOT, "package.json"), "utf8"),
  );
  const autolinking = JSON.parse(
    readFileSync(
      resolve(repositoryRoot, PACKAGE_ROOT, "expo-module.config.json"),
      "utf8",
    ),
  );
  const metadataErrors = validatePackageMetadata(metadata, autolinking);
  if (metadataErrors.length > 0) {
    throw new Error(metadataErrors.sort().join("\n"));
  }
  const temporaryRoot = mkdtempSync(join(tmpdir(), "midnight-mobile-package-"));
  try {
    const builtFiles = buildReproducibly(repositoryRoot);
    const packed = packPackage(repositoryRoot, temporaryRoot);
    validateConsumerInstall(repositoryRoot, temporaryRoot, packed.tarball);
    return { builtFiles, packedFiles: packed.entries };
  } finally {
    removeTree(temporaryRoot);
  }
}

function main() {
  try {
    const result = runPackageCheck();
    console.log(
      `React Native package passed: reproducible-dist-files=${String(result.builtFiles)}, npm-files=${String(result.packedFiles)}, isolated-offline-install=passed, packed-declarations-with-root-locked-peers=passed`,
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
