import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  validateReleaseConfig,
  validateReleaseTag,
} from "./release-policy.mjs";

const REPOSITORY_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

function fail(message) {
  throw new Error(`release input gate: ${message}`);
}

function readJson(path) {
  return JSON.parse(readFileSync(join(REPOSITORY_ROOT, path), "utf8"));
}

function git(arguments_) {
  const result = spawnSync("git", arguments_, {
    cwd: REPOSITORY_ROOT,
    encoding: "utf8",
  });
  if (result.error !== undefined || result.status !== 0) {
    fail(`git ${arguments_.join(" ")} failed`);
  }
  return `${result.stdout}`.trim();
}

function parseArguments(arguments_) {
  const options = { tag: undefined };
  for (let index = 0; index < arguments_.length; index += 1) {
    const argument = arguments_[index];
    if (argument === "--tag") {
      options.tag = arguments_[index + 1];
      index += 1;
      if (options.tag === undefined) fail("--tag requires a value");
    } else {
      fail(`unsupported argument: ${argument}`);
    }
  }
  return options;
}

export function checkReleaseInputs(arguments_ = process.argv.slice(2)) {
  const options = parseArguments(arguments_);
  const config = readJson("scripts/release-config.json");
  const errors = validateReleaseConfig(config, {
    rootMetadata: readJson("package.json"),
    packageMetadata: readJson("packages/react-native/package.json"),
    nativeConfig: readJson("scripts/native-build-config.json"),
    cargoLock: readFileSync(join(REPOSITORY_ROOT, "Cargo.lock"), "utf8"),
    compatibilityDocument: readFileSync(
      join(REPOSITORY_ROOT, "README.md"),
      "utf8",
    ),
  });
  const tagsAtHead =
    options.tag === undefined
      ? []
      : git(["tag", "--points-at", "HEAD"]).split("\n").filter(Boolean);
  errors.push(...validateReleaseTag(options.tag, tagsAtHead, config));
  if (options.tag !== undefined) {
    const status = git(["status", "--porcelain=v1", "--untracked-files=no"]);
    if (status.length > 0) errors.push("tagged release checkout must be clean");
  }
  if (errors.length > 0) fail(errors.sort().join("\n"));
  console.log(
    `release inputs passed: package=${config.package.name}@${config.package.version}, tag=${
      options.tag ?? "source-check"
    }`,
  );
}

checkReleaseInputs();
