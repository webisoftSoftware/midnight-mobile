#!/usr/bin/env node
// Stages the pinned prover crates into an ignored fork directory, applies the
// instrumentation patches there, and points the workspace at the result.
//
// Measurement rig, not product code: nothing here runs in a shipped build. The
// staged tree lives under `.prover-fork/`, which is gitignored, and the override
// is written to `.cargo/config.toml` so `revert` restores the pinned crates by
// deleting two paths and nothing else.
//
// Modelled on the extension's `wasm-prover-fork` rig
// (`packages/wallet-runtime-browser/scripts/build-zkir-mt.mjs`), which is why
// the patch files here have the same shape as the ones next to it.

import { execFileSync, spawnSync } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import path from "node:path";
import process from "node:process";

const repositoryRoot = path.resolve(import.meta.dirname, "..", "..");
const forkRoot = path.join(repositoryRoot, ".prover-fork");
const cargoConfig = path.join(repositoryRoot, ".cargo", "config.toml");
const cargoLock = path.join(repositoryRoot, "Cargo.lock");
// Staging rewrites `Cargo.lock`: a path override drops the registry source and
// checksum lines for the crates it replaces. Keep a copy so `revert` restores the
// pinned lockfile exactly rather than leaving the developer to notice the drift.
const pristineLock = path.join(forkRoot, "Cargo.lock.pristine");
// Where the pinned crates were copied from, recorded at stage time. `capture`
// cannot ask Cargo: with the override in place, `cargo metadata` reports the fork
// as the crate's source, so a diff against it is always empty -- which silently
// produces a patch that stages nothing.
const stagedSources = path.join(forkRoot, "sources.json");
const patches = ["phase-counters.patch"];

// Crates the counters live in. The versions are whatever `Cargo.lock` resolved,
// read at run time rather than hard-coded so a dependency bump fails loudly on
// the patch instead of silently instrumenting the wrong source.
const instrumented = ["midnight-curves", "midnight-proofs"];

function run(command, args, options = {}) {
  return execFileSync(command, args, {
    cwd: repositoryRoot,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "inherit"],
    // `cargo metadata` for this workspace is several megabytes.
    maxBuffer: 64 * 1024 * 1024,
    ...options,
  });
}

function resolvedCrates() {
  const metadata = JSON.parse(
    run("cargo", ["metadata", "--format-version", "1", "--locked"]),
  );
  return instrumented.map((name) => {
    const selected = metadata.packages.filter((pkg) => pkg.name === name);
    if (selected.length !== 1) {
      throw new Error(
        `Cargo resolved ${String(selected.length)} versions of ${name}; ` +
          "the rig instruments exactly one.",
      );
    }
    const [pkg] = selected;
    return {
      name,
      version: pkg.version,
      source: path.dirname(pkg.manifest_path),
    };
  });
}

function stage() {
  // Resolve against the pinned crates, not a fork left over from a previous run:
  // a stale override makes `cargo metadata` fail before it can report anything.
  revert();
  const crates = resolvedCrates();
  rmSync(forkRoot, { recursive: true, force: true });
  mkdirSync(forkRoot, { recursive: true });
  cpSync(cargoLock, pristineLock);
  writeFileSync(stagedSources, JSON.stringify(crates, null, 2));
  for (const crate of crates) {
    const destination = path.join(forkRoot, `${crate.name}-${crate.version}`);
    cpSync(crate.source, destination, { recursive: true });
    // Registry checkouts are read-only; the fork has to be writable to patch.
    run("chmod", ["-R", "u+w", destination]);
  }

  // Bootstrap escape hatch for regenerating the patches themselves: stage the
  // pinned sources, edit them, then `capture`.
  if (process.env.ONEAM_STAGE_WITHOUT_PATCHES === "1") {
    process.stdout.write("staged without patches; edit and then run capture\n");
    writeOverride(crates);
    return;
  }

  for (const patch of patches) {
    const file = path.join(import.meta.dirname, patch);
    if (!existsSync(file)) throw new Error(`Missing patch ${file}`);
    if (readFileSync(file, "utf8").trim().length === 0) {
      throw new Error(
        `${file} is empty; regenerate it with ONEAM_STAGE_WITHOUT_PATCHES=1 ` +
          "stage, then capture.",
      );
    }
    // Dry run first so a version bump reports the rejection before any file is
    // half-applied and the fork has to be rebuilt from scratch.
    for (const dry of [true, false]) {
      run(
        "patch",
        [
          ...(dry ? ["--dry-run"] : []),
          "--batch",
          "--forward",
          "-p1",
          "-i",
          file,
        ],
        {
          cwd: forkRoot,
          stdio: dry ? ["ignore", "ignore", "inherit"] : "inherit",
        },
      );
    }
  }

  writeOverride(crates);
  const versions = crates
    .map((crate) => `${crate.name} ${crate.version}`)
    .join(", ");
  process.stdout.write(`staged instrumented ${versions}\n`);
}

function writeOverride(crates) {
  mkdirSync(path.dirname(cargoConfig), { recursive: true });
  const overrides = crates
    .map(
      (crate) =>
        // Relative to the directory holding `.cargo`, i.e. the repository root.
        `${crate.name} = { path = ".prover-fork/${crate.name}-${crate.version}" }`,
    )
    .join("\n");
  writeFileSync(
    cargoConfig,
    "# Generated by scripts/mobile-prover-instrumentation/stage.mjs.\n" +
      "# Measurement rig only -- run `stage.mjs revert` before building a shipped\n" +
      "# artifact, and never commit this file.\n" +
      `[patch.crates-io]\n${overrides}\n`,
  );
}

function revert() {
  if (existsSync(pristineLock)) cpSync(pristineLock, cargoLock);
  rmSync(forkRoot, { recursive: true, force: true });
  if (existsSync(cargoConfig)) {
    const contents = readFileSync(cargoConfig, "utf8");
    if (!contents.includes("mobile-prover-instrumentation")) {
      throw new Error(
        `${cargoConfig} was not written by this rig; remove the override by hand.`,
      );
    }
    rmSync(cargoConfig);
  }
  process.stdout.write("reverted to the pinned prover crates\n");
}

// Regenerates the patch from the staged tree, so an edit made while measuring
// becomes a committed patch instead of being lost -- which is how the Phase A
// edits behind the numbers in `docs/performance/mobile/` went missing.
function capture() {
  if (!existsSync(stagedSources)) {
    throw new Error(`Nothing staged: ${stagedSources} is missing.`);
  }
  const chunks = [];
  for (const crate of JSON.parse(readFileSync(stagedSources, "utf8"))) {
    const staged = path.join(forkRoot, `${crate.name}-${crate.version}`);
    if (!existsSync(staged)) throw new Error(`Nothing staged at ${staged}`);
    // `diff` exits 1 when the trees differ, which is the expected outcome here.
    const result = spawnSync(
      "diff",
      ["-ruN", "--exclude=.cargo-ok", crate.source, staged],
      { cwd: repositoryRoot, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
    );
    if (result.status !== 0 && result.status !== 1) {
      throw new Error(
        `diff failed for ${crate.name}: ${String(result.stderr)}`,
      );
    }
    // Rewrite both sides onto the `-p1` layout the fork root applies under.
    const prefix = `${crate.name}-${crate.version}`;
    chunks.push(
      result.stdout
        .replaceAll(`${crate.source}/`, `a/${prefix}/`)
        .replaceAll(`${staged}/`, `b/${prefix}/`),
    );
  }
  process.stdout.write(chunks.join(""));
}

const [action = "stage"] = process.argv.slice(2);
const actions = { stage, revert, capture };
const selected = actions[action];
if (!selected) {
  process.stderr.write(
    `Unknown action ${action}; expected stage, revert, or capture.\n`,
  );
  process.exit(1);
}
selected();
