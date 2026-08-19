import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import {
  mkdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const REPOSITORY_ROOT = resolve(
  dirname(fileURLToPath(import.meta.url)),
  "../..",
);
const OUTPUT = join(REPOSITORY_ROOT, "artifacts/release-inputs");

function fail(message) {
  throw new Error(`release source gate: ${message}`);
}

function run(command, arguments_) {
  const result = spawnSync(command, arguments_, {
    cwd: REPOSITORY_ROOT,
    encoding: "utf8",
  });
  if (result.error !== undefined || result.status !== 0) {
    fail(`${command} ${arguments_.join(" ")} failed`);
  }
  return `${result.stdout}`.trim();
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function stableJson(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function tagArgument(arguments_) {
  if (arguments_.length !== 2 || arguments_[0] !== "--tag") {
    fail("usage: prepare-release-source.mjs --tag <tag>");
  }
  const tag = arguments_[1];
  if (!/^v\d+\.\d+\.\d+-alpha\.\d+$/u.test(tag)) {
    fail("tag must be an alpha semantic version");
  }
  return tag;
}

export function prepareReleaseSource(arguments_ = process.argv.slice(2)) {
  const tag = tagArgument(arguments_);
  const config = JSON.parse(
    readFileSync(
      join(REPOSITORY_ROOT, "scripts/release/release-config.json"),
      "utf8",
    ),
  );
  if (tag !== config.package.gitTag) fail("tag does not match release config");
  const commit = run("git", ["rev-parse", `${tag}^{commit}`]);
  if (commit !== run("git", ["rev-parse", "HEAD"])) {
    fail("tag does not point at the checked-out commit");
  }

  rmSync(OUTPUT, { force: true, recursive: true });
  mkdirSync(OUTPUT, { recursive: true });
  const archiveName = `midnight-mobile-${tag}.tar.gz`;
  const archivePath = join(OUTPUT, archiveName);
  run("git", [
    "archive",
    "--format=tar.gz",
    `--prefix=midnight-mobile-${tag}/`,
    `--output=${archivePath}`,
    commit,
  ]);
  const archiveEntry = {
    path: `artifacts/release-inputs/${archiveName}`,
    sha256: sha256(archivePath),
    size: statSync(archivePath).size,
  };
  const metadataPath = join(OUTPUT, "tag-metadata.json");
  writeFileSync(
    metadataPath,
    stableJson({
      schemaVersion: 1,
      tag,
      gitCommit: commit,
      commitTimestamp: run("git", ["show", "-s", "--format=%cI", commit]),
      package: config.package,
      compatibility: config.compatibility,
      sourceArchive: archiveEntry,
    }),
  );
  const metadataEntry = {
    path: "artifacts/release-inputs/tag-metadata.json",
    sha256: sha256(metadataPath),
    size: statSync(metadataPath).size,
  };
  writeFileSync(
    join(OUTPUT, "SHA256SUMS.json"),
    stableJson({
      schemaVersion: 1,
      algorithm: "SHA-256",
      entries: [archiveEntry, metadataEntry],
    }),
  );
  console.log(
    `release source passed: tag=${tag}, commit=${commit}, archive=${archiveEntry.sha256}`,
  );
}

prepareReleaseSource();
