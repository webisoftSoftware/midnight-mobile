import { spawnSync } from "node:child_process";
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  realpathSync,
  rmSync,
  statSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { listRepositoryFiles } from "../quality/quality-utils.mjs";
import {
  findSecretErrors,
  validateCargoDependencies,
  validateNpmDependencies,
  validateSecurityPolicy,
} from "./security-gates.mjs";

const REPOSITORY_ROOT = resolve(
  dirname(fileURLToPath(import.meta.url)),
  "../..",
);
const POLICY_PATH = join(
  REPOSITORY_ROOT,
  "scripts/security/security-policy.json",
);
const TARBALL_DIRECTORY = join(REPOSITORY_ROOT, "artifacts/native/npm");

function fail(errors) {
  throw new Error(`security gate failed:\n${errors.sort().join("\n")}`);
}

function run(command, arguments_, options = {}) {
  const result = spawnSync(command, arguments_, {
    cwd: options.cwd ?? REPOSITORY_ROOT,
    encoding: "utf8",
    maxBuffer: 128 * 1024 * 1024,
  });
  if (result.error !== undefined || result.status !== 0) {
    const detail = `${result.stderr ?? result.stdout ?? ""}`.trim();
    throw new Error(
      `${command} failed${detail.length === 0 ? "" : `: ${detail}`}`,
    );
  }
  return `${result.stdout ?? ""}`;
}

function collectFiles(root, path = root, output = []) {
  for (const entry of readdirSync(path).sort()) {
    const candidate = join(path, entry);
    if (statSync(candidate).isDirectory()) {
      collectFiles(root, candidate, output);
    } else {
      output.push({
        path: relative(root, candidate).replaceAll("\\", "/"),
        contents: readFileSync(candidate).toString("latin1"),
      });
    }
  }
  return output;
}

function repositoryEntries() {
  return listRepositoryFiles().map((path) => ({
    path,
    contents: readFileSync(join(REPOSITORY_ROOT, path)).toString("latin1"),
  }));
}

function historyEntry() {
  return {
    path: "git-history",
    contents: run("git", [
      "log",
      "--all",
      "--format=",
      "--no-ext-diff",
      "--patch",
      "--",
      ".",
    ]),
  };
}

function packedEntries(temporaryRoot) {
  if (!existsSync(TARBALL_DIRECTORY)) {
    throw new Error("security gate failed: native npm artifact is missing");
  }
  const tarballs = readdirSync(TARBALL_DIRECTORY).filter((name) =>
    name.endsWith(".tgz"),
  );
  if (tarballs.length !== 1) {
    throw new Error("security gate failed: expected exactly one npm tarball");
  }
  const extracted = join(temporaryRoot, "package");
  run("tar", [
    "-xzf",
    join(TARBALL_DIRECTORY, tarballs[0]),
    "-C",
    temporaryRoot,
  ]);
  return collectFiles(extracted);
}

function cargoMetadata() {
  return JSON.parse(
    run("cargo", [
      "metadata",
      "--locked",
      "--offline",
      "--format-version",
      "1",
    ]),
  );
}

export function checkSecurity(arguments_ = process.argv.slice(2)) {
  const sourceOnly =
    arguments_.length === 1 && arguments_[0] === "--source-only";
  if (arguments_.length > (sourceOnly ? 1 : 0)) {
    throw new Error("security gate failed: unsupported arguments");
  }
  const policy = JSON.parse(readFileSync(POLICY_PATH, "utf8"));
  const packageMetadata = JSON.parse(
    readFileSync(join(REPOSITORY_ROOT, "package.json"), "utf8"),
  );
  const lockfile = JSON.parse(
    readFileSync(join(REPOSITORY_ROOT, "package-lock.json"), "utf8"),
  );
  const temporaryRoot = realpathSync(
    mkdtempSync(join(tmpdir(), "midnight-security-")),
  );
  try {
    const packed = sourceOnly ? [] : packedEntries(temporaryRoot);
    const scanned = [...repositoryEntries(), historyEntry(), ...packed];
    const errors = [
      ...validateSecurityPolicy(policy),
      ...validateNpmDependencies(packageMetadata, lockfile),
      ...validateCargoDependencies(cargoMetadata(), policy),
      ...findSecretErrors(scanned),
    ];
    if (errors.length > 0) fail(errors);
    console.log(
      `security gate passed: repository+history+tarball=${String(
        scanned.length,
      )} entries, mode=${sourceOnly ? "source-only" : "complete"}, npm=${String(
        Object.keys(lockfile.packages).length,
      )} components, cargo=${String(cargoMetadata().packages.length)} components`,
    );
  } finally {
    rmSync(temporaryRoot, { force: true, recursive: true });
  }
}

checkSecurity();
