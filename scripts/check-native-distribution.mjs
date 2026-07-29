import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  symlinkSync,
  utimesSync,
  writeFileSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { dirname, join, relative, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const PACKAGE_ROOT = "packages/react-native";
const ARTIFACT_ROOT = "artifacts/native";
const LIBRARY = "libmidnight_native_runtime.so";
const ANDROID_ABIS = ["arm64-v8a", "x86_64"];
const PEERS = ["expo", "expo-modules-core", "react", "react-native"];

function fail(message) {
  throw new Error(`Native distribution gate: ${message}`);
}

function run(command, argumentsList, options = {}) {
  const result = spawnSync(command, argumentsList, {
    cwd: options.cwd,
    encoding: "utf8",
    env: options.env ?? process.env,
    maxBuffer: 64 * 1024 * 1024,
    stdio: options.inherit === true ? "inherit" : "pipe",
  });
  if (result.error !== undefined) fail(result.error.message);
  if (result.status !== 0) {
    const detail = `${result.stderr ?? result.stdout ?? ""}`.trim();
    fail(`${command} failed${detail.length === 0 ? "" : `:\n${detail}`}`);
  }
  return `${result.stdout ?? ""}`;
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}
function collectFiles(root, path = root, output = []) {
  if (!existsSync(path)) return output;
  for (const entry of readdirSync(path).sort()) {
    const candidate = resolve(path, entry);
    if (statSync(candidate).isDirectory()) {
      collectFiles(root, candidate, output);
    } else {
      output.push(relative(root, candidate).replaceAll("\\", "/"));
    }
  }
  return output;
}
function requireNativeInputs(repositoryRoot, configuration) {
  const packageRoot = resolve(repositoryRoot, PACKAGE_ROOT);
  const metadata = JSON.parse(
    readFileSync(resolve(packageRoot, "package.json"), "utf8"),
  );
  if (metadata.scripts?.postinstall !== undefined) {
    fail("npm package must not download binaries from postinstall");
  }
  const gradle = readFileSync(
    resolve(packageRoot, "android/build.gradle"),
    "utf8",
  );
  if (
    !gradle.includes(`implementation '${configuration.android.jna.coordinate}'`)
  ) {
    fail(
      "Android Gradle JNA coordinate does not match the pinned configuration",
    );
  }
  if (/\b(?:cargo|rustc)\b/u.test(gradle)) {
    fail("Android package Gradle configuration invokes the Rust toolchain");
  }
  const android = ANDROID_ABIS.map((abi) =>
    resolve(packageRoot, "android/src/main/jniLibs", abi, LIBRARY),
  );
  const xcframework = resolve(
    packageRoot,
    "ios/build",
    `${configuration.apple.frameworkName}.xcframework`,
  );
  const apple = collectFiles(xcframework)
    .filter((path) =>
      path.endsWith(`.framework/${configuration.apple.frameworkName}`),
    )
    .map((path) => resolve(xcframework, path));
  const missing = [...android, xcframework].filter((path) => !existsSync(path));
  if (missing.length > 0)
    fail(`native inputs are missing: ${missing.join(", ")}`);
  if (apple.length !== 2) {
    fail(
      `expected two Apple framework binaries; received ${String(apple.length)}`,
    );
  }
  return { android, apple, xcframework };
}
function normalizeTree(path) {
  const epoch = new Date(1_785_340_800_000);
  const directories = [];
  const visit = (directory) => {
    directories.push(directory);
    for (const entry of readdirSync(directory).sort()) {
      const candidate = resolve(directory, entry);
      if (statSync(candidate).isDirectory()) visit(candidate);
      else utimesSync(candidate, epoch, epoch);
    }
  };
  visit(path);
  directories
    .reverse()
    .forEach((directory) => utimesSync(directory, epoch, epoch));
}
function zipAndroidPayload(repositoryRoot, source, destination, stagingRoot) {
  rmSync(stagingRoot, { force: true, recursive: true });
  mkdirSync(stagingRoot, { recursive: true });
  cpSync(source, resolve(stagingRoot, "jniLibs"), { recursive: true });
  normalizeTree(stagingRoot);
  const files = collectFiles(stagingRoot);
  rmSync(destination, { force: true });
  mkdirSync(dirname(destination), { recursive: true });
  run("/usr/bin/zip", ["-X", "-q", destination, ...files], {
    cwd: stagingRoot,
  });
  return sha256(destination);
}
function createAndroidArchive(repositoryRoot, temporaryRoot) {
  const source = resolve(
    repositoryRoot,
    PACKAGE_ROOT,
    "android/src/main/jniLibs",
  );
  const output = resolve(repositoryRoot, ARTIFACT_ROOT, "android-jniLibs.zip");
  const first = resolve(temporaryRoot, "android-first.zip");
  const firstHash = zipAndroidPayload(
    repositoryRoot,
    source,
    first,
    resolve(temporaryRoot, "android-first"),
  );
  const secondHash = zipAndroidPayload(
    repositoryRoot,
    source,
    output,
    resolve(temporaryRoot, "android-second"),
  );
  if (firstHash !== secondHash)
    fail("Android standalone archive is not reproducible");
  return output;
}
function parsePackReport(stdout) {
  let report;
  try {
    report = JSON.parse(stdout)[0];
  } catch {
    fail("npm pack returned invalid JSON");
  }
  if (typeof report?.filename !== "string" || !Array.isArray(report.files)) {
    fail("npm pack report is incomplete");
  }
  return report;
}
function packOnce(repositoryRoot, destination) {
  mkdirSync(destination, { recursive: true });
  const stdout = run(
    "npm",
    [
      "pack",
      "--json",
      "--pack-destination",
      destination,
      "--cache",
      resolve(destination, "npm-cache"),
      resolve(repositoryRoot, PACKAGE_ROOT),
    ],
    { cwd: repositoryRoot },
  );
  const report = parsePackReport(stdout);
  return { entries: report.files, path: resolve(destination, report.filename) };
}
function validatePackedNative(entries, configuration) {
  const paths = entries.map((entry) => entry.path);
  for (const abi of ANDROID_ABIS) {
    const expected = `android/src/main/jniLibs/${abi}/${LIBRARY}`;
    if (!paths.includes(expected)) fail(`npm tarball is missing ${expected}`);
  }
  const appleBinaries = paths.filter((path) =>
    path.endsWith(`.framework/${configuration.apple.frameworkName}`),
  );
  if (appleBinaries.length !== 2) {
    fail("npm tarball must contain exactly two Apple framework binaries");
  }
}
function createTarball(repositoryRoot, temporaryRoot, configuration) {
  const first = packOnce(repositoryRoot, resolve(temporaryRoot, "pack-first"));
  const outputRoot = resolve(repositoryRoot, ARTIFACT_ROOT, "npm");
  rmSync(outputRoot, { force: true, recursive: true });
  const second = packOnce(repositoryRoot, outputRoot);
  validatePackedNative(second.entries, configuration);
  if (sha256(first.path) !== sha256(second.path)) {
    fail("npm tarball is not byte-for-byte reproducible");
  }
  return second.path;
}

function writeChecksumManifest(repositoryRoot, inputs, archives, tarball) {
  const paths = [...inputs.android, ...inputs.apple, ...archives, tarball];
  const entries = paths
    .map((path) => ({
      path: relative(repositoryRoot, path).replaceAll("\\", "/"),
      sha256: sha256(path),
      size: statSync(path).size,
    }))
    .sort((left, right) => left.path.localeCompare(right.path));
  const manifest = {
    schemaVersion: 1,
    algorithm: "SHA-256",
    entries,
  };
  const path = resolve(repositoryRoot, ARTIFACT_ROOT, "SHA256SUMS.json");
  writeFileSync(path, `${JSON.stringify(manifest, null, 2)}\n`);
  const reread = JSON.parse(readFileSync(path, "utf8"));
  for (const entry of reread.entries) {
    const artifact = resolve(repositoryRoot, entry.path);
    if (sha256(artifact) !== entry.sha256) {
      fail(`checksum verification failed: ${entry.path}`);
    }
  }
  return path;
}

function writeConsumerPackage(consumer, tarball) {
  const metadata = {
    name: "midnight-native-rustless-consumer",
    version: "0.0.0",
    private: true,
    main: "index.ts",
    dependencies: {
      "@1am/midnight-mobile": pathToFileURL(tarball).href,
    },
  };
  writeFileSync(
    resolve(consumer, "package.json"),
    `${JSON.stringify(metadata, null, 2)}\n`,
  );
}

function linkPeers(repositoryRoot, consumer) {
  for (const dependency of PEERS) {
    const destination = resolve(consumer, "node_modules", dependency);
    mkdirSync(dirname(destination), { recursive: true });
    symlinkSync(
      resolve(repositoryRoot, "node_modules", dependency),
      destination,
      "junction",
    );
  }
}

function prepareAndroidConsumer(repositoryRoot, temporaryRoot, tarball) {
  const consumer = resolve(temporaryRoot, "consumer");
  mkdirSync(consumer, { recursive: true });
  for (const path of ["App.tsx", "app.json", "index.ts", "src"]) {
    cpSync(
      resolve(repositoryRoot, "examples/expo", path),
      resolve(consumer, path),
      {
        recursive: true,
      },
    );
  }
  writeConsumerPackage(consumer, tarball);
  run(
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
    { cwd: consumer },
  );
  linkPeers(repositoryRoot, consumer);
  run(
    process.execPath,
    [
      resolve(repositoryRoot, "node_modules/expo/bin/cli"),
      "prebuild",
      "--platform",
      "android",
      "--no-install",
      "--clean",
    ],
    { cwd: consumer, inherit: true },
  );
  return consumer;
}

function rustlessEnvironment(repositoryRoot) {
  const androidHome = process.env.ANDROID_HOME ?? process.env.ANDROID_SDK_ROOT;
  if (androidHome === undefined) fail("ANDROID_HOME is required");
  const javaHome = resolveJava17(repositoryRoot);
  const path = [
    dirname(process.execPath),
    resolve(javaHome, "bin"),
    "/opt/homebrew/bin",
    "/usr/bin",
    "/bin",
    "/usr/sbin",
    "/sbin",
  ]
    .filter((entry, index, entries) => entries.indexOf(entry) === index)
    .join(":");
  const environment = {
    ...process.env,
    ANDROID_HOME: androidHome,
    ANDROID_SDK_ROOT: androidHome,
    JAVA_HOME: javaHome,
    PATH: path,
  };
  for (const executable of ["cargo", "rustc", "rustup"]) {
    const result = spawnSync(executable, ["--version"], {
      cwd: repositoryRoot,
      encoding: "utf8",
      env: environment,
    });
    if (result.error?.code !== "ENOENT") {
      fail(`${executable} must be unavailable in rustless PATH`);
    }
  }
  return environment;
}

function resolveJava17(repositoryRoot) {
  const candidates = [
    process.env.JAVA_HOME,
    "/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home",
    "/usr/local/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home",
  ].filter((candidate) => candidate !== undefined);
  for (const candidate of candidates) {
    const java = resolve(candidate, "bin/java");
    if (!existsSync(java)) continue;
    const result = spawnSync(java, ["-version"], {
      cwd: repositoryRoot,
      encoding: "utf8",
    });
    if (`${result.stderr ?? result.stdout ?? ""}`.includes('version "17.')) {
      return candidate;
    }
  }
  fail("a JDK 17 JAVA_HOME is required for the Android consumer build");
}

function validateAndroidApk(consumer, environment) {
  const android = resolve(consumer, "android");
  const gradleFiles = collectFiles(android)
    .filter((path) => /\.(?:gradle|gradle\.kts)$/u.test(path))
    .map((path) => readFileSync(resolve(android, path), "utf8"))
    .join("\n");
  if (/\b(?:cargo|rustc)\b/u.test(gradleFiles)) {
    fail("consumer Gradle configuration invokes the Rust toolchain");
  }
  run(
    resolve(android, "gradlew"),
    [
      "assembleRelease",
      "--offline",
      "--no-daemon",
      "--stacktrace",
      "-PreactNativeArchitectures=arm64-v8a,x86_64",
    ],
    { cwd: android, env: environment, inherit: true },
  );
  const apk = resolve(android, "app/build/outputs/apk/release/app-release.apk");
  if (!existsSync(apk)) fail("Android release APK was not produced");
  const listing = run("/usr/bin/unzip", ["-Z1", apk], { cwd: android });
  const runtimeAbis = listing
    .split("\n")
    .filter((path) => path.startsWith("lib/") && path.endsWith(`/${LIBRARY}`))
    .map((path) => path.split("/")[1])
    .filter((abi) => abi !== undefined)
    .sort();
  if (JSON.stringify(runtimeAbis) !== JSON.stringify(ANDROID_ABIS)) {
    fail(`Android release APK runtime ABIs are ${runtimeAbis.join(",")}`);
  }
  const packagedAbis = [
    ...new Set(
      listing
        .split("\n")
        .map((path) => /^lib\/([^/]+)\//u.exec(path)?.[1])
        .filter((abi) => abi !== undefined),
    ),
  ].sort();
  if (JSON.stringify(packagedAbis) !== JSON.stringify(ANDROID_ABIS)) {
    fail(`Android release APK native ABIs are ${packagedAbis.join(",")}`);
  }
}

function validateResolvedJna(configuration) {
  const root = resolve(
    homedir(),
    ".gradle/caches/modules-2/files-2.1/net.java.dev.jna/jna/5.17.0",
  );
  const archives = collectFiles(root)
    .filter((path) => path.endsWith("/jna-5.17.0.aar"))
    .map((path) => resolve(root, path));
  if (archives.length !== 1) {
    fail(
      `expected one resolved JNA 5.17.0 AAR; received ${String(archives.length)}`,
    );
  }
  if (sha256(archives[0]) !== configuration.android.jna.sha256) {
    fail("resolved JNA 5.17.0 AAR checksum drifted");
  }
}

function validateRustlessAndroid(
  repositoryRoot,
  temporaryRoot,
  tarball,
  configuration,
) {
  const consumer = prepareAndroidConsumer(
    repositoryRoot,
    temporaryRoot,
    tarball,
  );
  validateAndroidApk(consumer, rustlessEnvironment(repositoryRoot));
  validateResolvedJna(configuration);
}

export function checkNativeDistribution(repositoryRoot = process.cwd()) {
  const configuration = JSON.parse(
    readFileSync(
      resolve(repositoryRoot, "scripts/native-build-config.json"),
      "utf8",
    ),
  );
  const temporaryRoot = mkdtempSync(join(tmpdir(), "midnight-native-check-"));
  try {
    const inputs = requireNativeInputs(repositoryRoot, configuration);
    const androidArchive = createAndroidArchive(repositoryRoot, temporaryRoot);
    const appleArchive = resolve(
      repositoryRoot,
      "artifacts/apple",
      `${configuration.apple.frameworkName}.xcframework.zip`,
    );
    if (!existsSync(appleArchive)) fail("Apple standalone archive is missing");
    const tarball = createTarball(repositoryRoot, temporaryRoot, configuration);
    const manifest = writeChecksumManifest(
      repositoryRoot,
      inputs,
      [androidArchive, appleArchive],
      tarball,
    );
    validateRustlessAndroid(
      repositoryRoot,
      temporaryRoot,
      tarball,
      configuration,
    );
    return { manifest, tarball };
  } finally {
    rmSync(temporaryRoot, { force: true, recursive: true });
  }
}

function main() {
  try {
    const result = checkNativeDistribution();
    console.log(
      `Native distribution passed: tarball=${relative(process.cwd(), result.tarball)}, checksums=${relative(process.cwd(), result.manifest)}, rustless-android-release=passed`,
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
