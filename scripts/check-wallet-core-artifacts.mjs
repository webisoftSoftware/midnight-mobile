import { spawnSync } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { extname, relative, resolve } from "node:path";
import { pathToFileURL } from "node:url";

import {
  ALLOWED_COMMAND_KINDS,
  findForbiddenContent,
} from "./check-wallet-core-boundary.mjs";

const TEXT_EXTENSIONS = new Set(
  ".js,.json,.kt,.map,.md,.modulemap,.podspec,.swift,.ts".split(","),
);
const RUST_ARTIFACT_PATTERN =
  /(?:^|\/)(?:lib)?midnight_native_runtime(?:[-.][^/]*)?\.(?:dll|dylib|so)$/u;
const NATIVE_PACKAGE_PATTERN =
  /\.(?:a|aar|dll|dylib|framework|so|xcframework)(?:\/|$)/u;

function artifactError(result, label) {
  if (result.error !== undefined) return `${label}: ${result.error.message}`;
  const detail = `${result.stderr ?? ""}`.trim();
  return `${label} failed${detail.length === 0 ? "" : `: ${detail}`}`;
}

function run(repositoryRoot, command, arguments_) {
  return spawnSync(command, arguments_, {
    cwd: repositoryRoot,
    encoding: "utf8",
    maxBuffer: 20 * 1024 * 1024,
  });
}

function collectFiles(path, output) {
  if (!existsSync(path)) return;
  const stat = statSync(path);
  if (stat.isDirectory()) {
    for (const entry of readdirSync(path)) {
      collectFiles(resolve(path, entry), output);
    }
  } else if (stat.isFile()) {
    output.push(path);
  }
}

function runTypeScriptCompiler(repositoryRoot, project, label, errors) {
  const compiler = resolve(repositoryRoot, "node_modules/typescript/bin/tsc");
  const result = run(repositoryRoot, process.execPath, [
    compiler,
    "--project",
    project,
  ]);
  if (result.status !== 0) {
    errors.push(artifactError(result, label));
    return false;
  }
  return true;
}

function copyIfPresent(source, target) {
  if (!existsSync(source)) return;
  mkdirSync(resolve(target, ".."), { recursive: true });
  cpSync(source, target, { recursive: true });
}

export function resetGeneratedDirectory(path) {
  rmSync(path, { force: true, recursive: true });
}

export function extractAsciiStrings(bytes, minimumLength = 4) {
  const result = [];
  let current = "";
  for (const byte of bytes) {
    if (byte >= 0x20 && byte <= 0x7e) {
      current += String.fromCharCode(byte);
    } else {
      if (current.length >= minimumLength) result.push(current);
      current = "";
    }
  }
  if (current.length >= minimumLength) result.push(current);
  return result;
}

export function findPublicDeclarationErrors(source) {
  const errors = [];
  const match = /interface MidnightCommandResultMap\s*\{([\s\S]*?)^\}/mu.exec(
    source,
  );
  const kinds =
    match === null
      ? []
      : [...match[1].matchAll(/^\s+readonly ([A-Za-z][A-Za-z0-9]*):/gmu)].map(
          (item) => item[1],
        );
  if (
    kinds.length !== ALLOWED_COMMAND_KINDS.length ||
    kinds.some((kind, index) => kind !== ALLOWED_COMMAND_KINDS[index])
  ) {
    errors.push("public declaration result map must retain exactly 19 kinds");
  }
  if (match === null) {
    errors.push("public declaration result map is missing");
  } else if (/\bunknown\b/u.test(match[1])) {
    errors.push("public declaration command results must not contain unknown");
  }
  return errors;
}

export function findForbiddenBinaryContent(path, bytes) {
  return findForbiddenContent([
    { path, contents: Buffer.from(bytes).toString("latin1") },
  ]);
}

function scanDeclarations(repositoryRoot, errors) {
  resetGeneratedDirectory(
    resolve(repositoryRoot, "packages/react-native/.declaration-build"),
  );
  if (
    !runTypeScriptCompiler(
      repositoryRoot,
      "packages/react-native/tsconfig.declarations.json",
      "TypeScript declaration generation",
      errors,
    )
  ) {
    return 0;
  }
  const root = resolve(
    repositoryRoot,
    "packages/react-native/.declaration-build/src",
  );
  const paths = [];
  collectFiles(root, paths);
  const files = paths
    .filter((path) => path.endsWith(".d.ts"))
    .map((path) => ({
      path: relative(repositoryRoot, path).replaceAll("\\", "/"),
      contents: readFileSync(path, "utf8"),
    }));
  errors.push(...findForbiddenContent(files));
  const commands = files.find((file) => file.path.endsWith("/commands.d.ts"));
  if (commands === undefined) {
    errors.push("generated public commands declaration is missing");
  } else {
    errors.push(...findPublicDeclarationErrors(commands.contents));
  }
  return files.length;
}

function assembleStagingPackage(repositoryRoot, errors) {
  const packageRoot = resolve(repositoryRoot, "packages/react-native");
  resetGeneratedDirectory(resolve(packageRoot, ".staging-build"));
  if (
    !runTypeScriptCompiler(
      repositoryRoot,
      "packages/react-native/tsconfig.test.json",
      "TypeScript staging JavaScript generation",
      errors,
    )
  ) {
    return undefined;
  }
  const stagingRoot = resolve(packageRoot, ".staging-build/artifact-pack");
  mkdirSync(resolve(stagingRoot, "dist"), { recursive: true });
  const sourceRoots = [
    [resolve(packageRoot, ".staging-build/src"), ".js"],
    [resolve(packageRoot, ".declaration-build/src"), ".d.ts"],
  ];
  for (const [sourceRoot, suffix] of sourceRoots) {
    const paths = [];
    collectFiles(sourceRoot, paths);
    for (const path of paths.filter((candidate) =>
      candidate.endsWith(suffix),
    )) {
      copyIfPresent(
        path,
        resolve(stagingRoot, "dist", relative(sourceRoot, path)),
      );
    }
  }
  for (const path of [
    "README.md",
    "android/build.gradle",
    "android/generated",
    "android/src/main",
    "expo-module.config.json",
    "ios/ExpoMidnightNative.podspec",
    "ios/ExpoMidnightNativeModule.swift",
    "ios/generated",
  ]) {
    copyIfPresent(resolve(packageRoot, path), resolve(stagingRoot, path));
  }
  const metadata = JSON.parse(
    readFileSync(resolve(packageRoot, "package.json"), "utf8"),
  );
  metadata.files = [
    "dist",
    "README.md",
    "android",
    "expo-module.config.json",
    "ios",
  ];
  delete metadata.scripts;
  writeFileSync(
    resolve(stagingRoot, "package.json"),
    `${JSON.stringify(metadata, null, 2)}\n`,
  );
  return stagingRoot;
}

export function findStagingPackageErrors(entries) {
  const errors = [];
  const paths = entries
    .filter((entry) => typeof entry?.path === "string")
    .map((entry) => entry.path);
  for (const required of ["dist/index.js", "dist/index.d.ts"]) {
    if (!paths.includes(required)) {
      errors.push(`staging npm package is missing ${required}`);
    }
  }
  for (const path of paths) {
    if (NATIVE_PACKAGE_PATTERN.test(path)) {
      errors.push(`npm:${path}: native release binary is premature before M4`);
    }
  }
  return errors;
}

function scanPackage(repositoryRoot, errors) {
  const stagingRoot = assembleStagingPackage(repositoryRoot, errors);
  if (stagingRoot === undefined) return 0;
  const result = run(repositoryRoot, "npm", [
    "pack",
    "--dry-run",
    "--json",
    "--cache",
    resolve(repositoryRoot, "packages/react-native/.staging-build/npm-cache"),
    stagingRoot,
  ]);
  if (result.status !== 0) {
    errors.push(artifactError(result, "npm package inspection"));
    return 0;
  }
  let reports;
  try {
    reports = JSON.parse(result.stdout);
  } catch {
    errors.push("npm package inspection returned invalid JSON");
    return 0;
  }
  const entries = Array.isArray(reports?.[0]?.files) ? reports[0].files : [];
  errors.push(...findStagingPackageErrors(entries));
  const files = [];
  for (const entry of entries) {
    if (typeof entry?.path !== "string") continue;
    const path = resolve(stagingRoot, entry.path);
    if (
      existsSync(path) &&
      statSync(path).isFile() &&
      TEXT_EXTENSIONS.has(extname(path).toLowerCase())
    ) {
      files.push({
        path: `npm:${entry.path}`,
        contents: readFileSync(path, "utf8"),
      });
    }
  }
  errors.push(...findForbiddenContent(files));
  return entries.length;
}

export function selectRuntimeArtifacts(paths) {
  const candidates = paths.filter((path) =>
    RUST_ARTIFACT_PATTERN.test(path.replaceAll("\\", "/")),
  );
  const direct = candidates.filter(
    (path) => !path.replaceAll("\\", "/").includes("/deps/"),
  );
  return direct.length > 0 ? direct : candidates.slice(0, 1);
}

function scanRustArtifacts(repositoryRoot, errors) {
  const build = run(repositoryRoot, "cargo", [
    "build",
    "--locked",
    "--package",
    "midnight-native-runtime",
  ]);
  if (build.status !== 0) {
    errors.push(artifactError(build, "Rust runtime artifact build"));
    return 0;
  }
  const paths = [];
  collectFiles(resolve(repositoryRoot, "target/debug"), paths);
  const artifacts = selectRuntimeArtifacts(paths);
  if (artifacts.length === 0) {
    errors.push(
      "Rust runtime artifact build produced no scannable host library",
    );
  }
  for (const path of artifacts) {
    errors.push(
      ...findForbiddenBinaryContent(
        relative(repositoryRoot, path).replaceAll("\\", "/"),
        readFileSync(path),
      ),
    );
  }
  return artifacts.length;
}

export function runArtifactCheck(repositoryRoot = process.cwd()) {
  const errors = [];
  const declarations = scanDeclarations(repositoryRoot, errors);
  const packageEntries = scanPackage(repositoryRoot, errors);
  const rustArtifacts = scanRustArtifacts(repositoryRoot, errors);
  return { declarations, errors, packageEntries, rustArtifacts };
}

function main() {
  const rootIndex = process.argv.indexOf("--root");
  const repositoryRoot =
    rootIndex >= 0 && process.argv[rootIndex + 1] !== undefined
      ? resolve(process.argv[rootIndex + 1])
      : process.cwd();
  const result = runArtifactCheck(repositoryRoot);
  if (result.errors.length > 0) {
    result.errors.sort().forEach((error) => console.error(error));
    process.exitCode = 1;
    return;
  }
  console.log(
    `wallet-core artifacts passed: declarations=${String(result.declarations)}, npm-files=${String(result.packageEntries)}, rust-artifacts=${String(result.rustArtifacts)}`,
  );
}

const invokedPath = process.argv[1];
if (
  invokedPath !== undefined &&
  pathToFileURL(resolve(invokedPath)).href === import.meta.url
) {
  main();
}
