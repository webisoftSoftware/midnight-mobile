import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export const EXPECTED_ABI = Object.freeze([
  "apply_sync_batch",
  "begin_command",
  "cancel_operation",
  "close_wallet_session",
  "export_wallet_checkpoint",
  "get_wallet_snapshot",
  "open_wallet_session",
  "resume_operation",
]);
const SWIFT_MODULE = "MidnightMobileRuntime";
const SWIFT_FFI_FILENAME = "MidnightMobileRuntimeFFI";

const SCRIPT_DIRECTORY = dirname(fileURLToPath(import.meta.url));
const REPOSITORY_ROOT = resolve(SCRIPT_DIRECTORY, "..");
const RUNTIME_PACKAGE = "midnight-mobile-runtime";
const BINDGEN_PACKAGE = "midnight-native-bindgen";
const SWIFT_FILES = Object.freeze([
  "MidnightMobileRuntime.swift",
  "MidnightMobileRuntimeFFI.h",
  "MidnightMobileRuntimeFFI.modulemap",
]);
const KOTLIN_FILES = Object.freeze([
  "dev/oneam/midnightmobile/uniffi/midnight_mobile_runtime.kt",
]);
const SWIFT_TARGET = "packages/react-native/ios/generated";
const KOTLIN_TARGET = "packages/react-native/android/generated";

function fail(message) {
  throw new Error(`UniFFI binding gate: ${message}`);
}

export function swiftModuleContractErrors(configuration, swift, modulemap) {
  const errors = [];
  if (!/^ffi_module_name = "MidnightMobileRuntime"$/mu.test(configuration)) {
    errors.push("Swift ffi_module_name must match the framework module");
  }
  if (
    !/^ffi_module_filename = "MidnightMobileRuntimeFFI"$/mu.test(configuration)
  ) {
    errors.push("Swift ffi_module_filename must preserve reviewed filenames");
  }
  if (
    !swift.includes(
      `#if canImport(${SWIFT_MODULE})\nimport ${SWIFT_MODULE}\n#endif`,
    )
  ) {
    errors.push("generated Swift must import the framework module");
  }
  if (!modulemap.startsWith(`module ${SWIFT_MODULE} {`)) {
    errors.push("generated module map must declare the framework module name");
  }
  if (!modulemap.includes(`header "${SWIFT_FFI_FILENAME}.h"`)) {
    errors.push("generated module map must retain the reviewed FFI header");
  }
  return errors;
}

function run(command, argumentsList, capture = false) {
  const result = spawnSync(command, argumentsList, {
    cwd: REPOSITORY_ROOT,
    encoding: "utf8",
    stdio: capture ? "pipe" : "inherit",
  });
  if (result.error) {
    fail(`unable to execute ${command}: ${result.error.message}`);
  }
  if (result.status !== 0) {
    const detail = capture ? result.stderr.trim() : "";
    fail(
      `${command} exited with status ${String(result.status)}${detail ? `: ${detail}` : ""}`,
    );
  }
  return result.stdout;
}

function requireRepositoryInputs() {
  const required = [
    "Cargo.lock",
    "Cargo.toml",
    "crates/runtime/Cargo.toml",
    "crates/runtime/uniffi.toml",
    "tools/bindgen/Cargo.toml",
  ];
  for (const path of required) {
    if (!existsSync(join(REPOSITORY_ROOT, path))) {
      fail(`required input is missing: ${path}`);
    }
  }
}

function cargoTargetDirectory() {
  const output = run(
    "cargo",
    ["metadata", "--locked", "--no-deps", "--format-version", "1"],
    true,
  );
  let metadata;
  try {
    metadata = JSON.parse(output);
  } catch {
    fail("cargo metadata returned invalid JSON");
  }
  if (typeof metadata.target_directory !== "string") {
    fail("cargo metadata did not report target_directory");
  }
  return metadata.target_directory;
}

function runtimeLibrary(targetDirectory) {
  const debugDirectory = join(targetDirectory, "debug");
  const candidates = [
    join(debugDirectory, "libmidnight_mobile_runtime.dylib"),
    join(debugDirectory, "libmidnight_mobile_runtime.so"),
    join(debugDirectory, "midnight_mobile_runtime.dll"),
  ].filter(existsSync);
  if (candidates.length !== 1) {
    fail(
      `expected one runtime dynamic library, found ${String(candidates.length)}`,
    );
  }
  return candidates[0];
}

function generateLanguage(library, language, outputDirectory) {
  mkdirSync(outputDirectory, { recursive: true });
  run("cargo", [
    "run",
    "--locked",
    "--quiet",
    "--package",
    BINDGEN_PACKAGE,
    "--",
    "generate",
    library,
    "--language",
    language,
    "--out-dir",
    outputDirectory,
    "--no-format",
  ]);
  normalizeGeneratedTree(outputDirectory);
}

function listFiles(directory, prefix = "") {
  const entries = readdirSync(directory, { withFileTypes: true }).sort((a, b) =>
    a.name.localeCompare(b.name),
  );
  const files = [];
  for (const entry of entries) {
    const relativePath = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory()) {
      files.push(...listFiles(join(directory, entry.name), relativePath));
    } else if (entry.isFile()) {
      files.push(relativePath);
    } else {
      fail(`unsupported generated entry: ${relativePath}`);
    }
  }
  return files;
}

export function normalizeGeneratedText(contents) {
  const normalizedLines = contents
    .replaceAll("\r\n", "\n")
    .replaceAll("\r", "\n")
    .split("\n")
    .map((line) => line.replace(/[ \t]+$/u, ""));
  return `${normalizedLines.join("\n").replace(/\n+$/u, "")}\n`;
}

function normalizeGeneratedTree(directory) {
  for (const path of listFiles(directory)) {
    const file = join(directory, ...path.split("/"));
    const normalized = normalizeGeneratedText(readFileSync(file, "utf8"));
    writeFileSync(file, normalized, "utf8");
  }
}

function assertFileList(actual, expected, label) {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    fail(
      `${label} file set changed: expected ${expected.join(", ")}, received ${actual.join(", ")}`,
    );
  }
}

function assertTreesEqual(first, second, expected, label) {
  const firstFiles = listFiles(first);
  const secondFiles = listFiles(second);
  assertFileList(firstFiles, expected, label);
  assertFileList(secondFiles, expected, label);
  for (const path of expected) {
    const firstBytes = readFileSync(join(first, ...path.split("/")));
    const secondBytes = readFileSync(join(second, ...path.split("/")));
    if (!firstBytes.equals(secondBytes)) {
      fail(`${label} output is not reproducible: ${path}`);
    }
  }
}

function snakeCase(value) {
  return value.replaceAll(/([a-z0-9])([A-Z])/gu, "$1_$2").toLowerCase();
}

function matches(contents, pattern) {
  return [...contents.matchAll(pattern)].map((match) => match[1]);
}

export function extractGeneratedAbi(swift, kotlin, header) {
  const swiftNames = matches(swift, /^public func ([A-Za-z][A-Za-z0-9]*)\(/gmu)
    .filter((name) => !name.startsWith("Ffi") && !name.startsWith("uniffi"))
    .map(snakeCase);
  const kotlinNames = matches(
    kotlin,
    /^\s*@Throws\(MidnightRuntimeException::class\) fun `([^`]+)`/gmu,
  ).map(snakeCase);
  const headerNames = matches(
    header,
    /\buniffi_midnight_mobile_runtime_fn_func_([a-z0-9_]+)\(/gu,
  );
  return { swiftNames, kotlinNames, headerNames };
}

function uniqueSorted(values) {
  return [...new Set(values)].sort();
}

export function assertExactGeneratedAbi(swift, kotlin, header) {
  const generated = extractGeneratedAbi(swift, kotlin, header);
  for (const [language, names] of Object.entries(generated)) {
    const actual = uniqueSorted(names);
    if (JSON.stringify(actual) !== JSON.stringify(EXPECTED_ABI)) {
      fail(
        `${language} ABI changed: expected ${EXPECTED_ABI.join(", ")}, received ${actual.join(", ")}`,
      );
    }
    if (names.length !== EXPECTED_ABI.length) {
      fail(`${language} ABI contains duplicate function declarations`);
    }
  }
}

function assertAbi(generatedRoot) {
  const swiftDirectory = join(generatedRoot, "swift");
  const kotlinDirectory = join(generatedRoot, "kotlin");
  const swift = readFileSync(join(swiftDirectory, SWIFT_FILES[0]), "utf8");
  const header = readFileSync(join(swiftDirectory, SWIFT_FILES[1]), "utf8");
  const modulemap = readFileSync(join(swiftDirectory, SWIFT_FILES[2]), "utf8");
  assertExactGeneratedAbi(
    swift,
    readFileSync(join(kotlinDirectory, ...KOTLIN_FILES[0].split("/")), "utf8"),
    header,
  );
  const moduleErrors = swiftModuleContractErrors(
    readFileSync(join(REPOSITORY_ROOT, "crates/runtime/uniffi.toml"), "utf8"),
    swift,
    modulemap,
  );
  if (moduleErrors.length > 0) fail(moduleErrors.join("\n"));
}

function targetFiles(directory) {
  if (!existsSync(directory)) return [];
  return listFiles(directory).filter((path) => path !== "README.md");
}

function writeOrCheckTree(source, targetRelative, expected, write) {
  const target = join(REPOSITORY_ROOT, targetRelative);
  if (write) {
    mkdirSync(target, { recursive: true });
    for (const path of expected) {
      const destination = join(target, ...path.split("/"));
      mkdirSync(dirname(destination), { recursive: true });
      copyFileSync(join(source, ...path.split("/")), destination);
    }
  }
  assertFileList(targetFiles(target), expected, `${targetRelative} committed`);
  for (const path of expected) {
    const generated = readFileSync(join(source, ...path.split("/")));
    const committed = readFileSync(join(target, ...path.split("/")));
    if (!generated.equals(committed)) {
      fail(`committed binding is stale: ${targetRelative}/${path}`);
    }
  }
}

function generateRun(root, library) {
  generateLanguage(library, "swift", join(root, "swift"));
  generateLanguage(library, "kotlin", join(root, "kotlin"));
}

function parseArguments(argumentsList) {
  if (argumentsList.length === 0) return { write: false };
  if (argumentsList.length === 1 && argumentsList[0] === "--write") {
    return { write: true };
  }
  fail(`unsupported arguments: ${argumentsList.join(" ")}`);
}

export function runBindingGate(argumentsList = process.argv.slice(2)) {
  const { write } = parseArguments(argumentsList);
  requireRepositoryInputs();
  run("cargo", ["build", "--locked", "--package", RUNTIME_PACKAGE]);
  const library = runtimeLibrary(cargoTargetDirectory());
  const temporaryRoot = mkdtempSync(join(tmpdir(), "midnight-uniffi-"));
  try {
    const first = join(temporaryRoot, "first");
    const second = join(temporaryRoot, "second");
    generateRun(first, library);
    generateRun(second, library);
    assertTreesEqual(
      join(first, "swift"),
      join(second, "swift"),
      SWIFT_FILES,
      "Swift",
    );
    assertTreesEqual(
      join(first, "kotlin"),
      join(second, "kotlin"),
      KOTLIN_FILES,
      "Kotlin",
    );
    assertAbi(first);
    writeOrCheckTree(join(first, "swift"), SWIFT_TARGET, SWIFT_FILES, write);
    writeOrCheckTree(join(first, "kotlin"), KOTLIN_TARGET, KOTLIN_FILES, write);
  } finally {
    rmSync(temporaryRoot, { recursive: true, force: true });
  }
  console.log(
    `UniFFI bindings are reproducible with exactly ${String(EXPECTED_ABI.length)} functions`,
  );
}

const invokedPath = process.argv[1];
if (
  invokedPath !== undefined &&
  pathToFileURL(resolve(invokedPath)).href === import.meta.url
) {
  try {
    runBindingGate();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
