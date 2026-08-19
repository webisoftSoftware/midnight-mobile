import { spawnSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { listRepositoryFiles } from "./quality-utils.mjs";
import {
  fingerprintEvidence,
  renderReleaseEvidence,
} from "./release-evidence.mjs";
import { sourceTreeFingerprint } from "./release-policy.mjs";

const REPOSITORY_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const OUTPUT = join(REPOSITORY_ROOT, "artifacts/release");

function readJson(path) {
  return JSON.parse(readFileSync(join(REPOSITORY_ROOT, path), "utf8"));
}

function run(command, arguments_) {
  const result = spawnSync(command, arguments_, {
    cwd: REPOSITORY_ROOT,
    encoding: "utf8",
    maxBuffer: 128 * 1024 * 1024,
  });
  if (result.error !== undefined || result.status !== 0) {
    throw new Error(`${command} ${arguments_.join(" ")} failed`);
  }
  return `${result.stdout}`.trim();
}

function inputs() {
  const releaseInputManifest = join(
    REPOSITORY_ROOT,
    "artifacts/release-inputs/SHA256SUMS.json",
  );
  const releaseInputEntries = existsSync(releaseInputManifest)
    ? JSON.parse(readFileSync(releaseInputManifest, "utf8")).entries
    : [];
  const nativeEntries = readJson(
    "artifacts/native/SHA256SUMS.json",
  ).entries.filter((entry) => entry.path.startsWith("artifacts/"));
  return {
    config: readJson("scripts/release-config.json"),
    npmLockfile: readJson("package-lock.json"),
    cargoMetadata: JSON.parse(
      run("cargo", [
        "metadata",
        "--locked",
        "--offline",
        "--format-version",
        "1",
      ]),
    ),
    artifactEntries: [...nativeEntries, ...releaseInputEntries],
    context: {
      gitCommit: run("git", ["rev-parse", "HEAD"]),
      commitTimestamp: run("git", ["show", "-s", "--format=%cI", "HEAD"]),
      sourceTreeSha256: sourceTreeFingerprint(
        REPOSITORY_ROOT,
        listRepositoryFiles(),
      ),
    },
  };
}

export function buildReleaseEvidence() {
  const releaseInputs = inputs();
  const first = renderReleaseEvidence(releaseInputs);
  const second = renderReleaseEvidence(releaseInputs);
  const firstFingerprint = fingerprintEvidence(first);
  const secondFingerprint = fingerprintEvidence(second);
  if (firstFingerprint !== secondFingerprint) {
    throw new Error("release evidence is not reproducible");
  }
  rmSync(OUTPUT, { force: true, recursive: true });
  mkdirSync(OUTPUT, { recursive: true });
  for (const [name, contents] of Object.entries(first)) {
    writeFileSync(join(OUTPUT, name), contents);
  }
  console.log(
    `release evidence passed: files=${String(
      Object.keys(first).length,
    )}, sha256=${firstFingerprint}`,
  );
}

buildReleaseEvidence();
