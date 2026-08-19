import { existsSync, readdirSync } from "node:fs";
import { resolve } from "node:path";

import { loadCoveragePolicy } from "./coverage-policy.mjs";
import {
  listRepositoryFiles,
  printNotApplicable,
  runCommand,
} from "./quality-utils.mjs";

const files = listRepositoryFiles();
const hasRustWorkstream = files.some(
  (file) =>
    file === "Cargo.toml" ||
    file.endsWith("/Cargo.toml") ||
    file.endsWith(".rs"),
);
const hasJavaScriptWorkstream = files.some(
  (file) =>
    /^(?:packages|examples)\/.+\.(?:js|jsx|ts|tsx)$/u.test(file) &&
    !file.endsWith(".d.ts"),
);
const reactNativePackage = "packages/react-native";
const expoExample = "examples/expo";
const hasReactNativeWorkstream = files.some((file) =>
  file.startsWith(`${reactNativePackage}/`),
);
const hasOtherJavaScriptWorkstream = files.some(
  (file) =>
    /^(?:packages|examples)\/.+\.(?:js|jsx|ts|tsx)$/u.test(file) &&
    !file.startsWith(`${reactNativePackage}/`) &&
    !file.startsWith(`${expoExample}/`) &&
    !file.endsWith(".d.ts"),
);
const hasExpoExample = files.some((file) => file.startsWith(`${expoExample}/`));
const scriptTests = files
  .filter((file) => /^scripts\/.+\.test\.mjs$/u.test(file))
  .sort();
const testMode = process.argv.includes("--coverage") ? "coverage" : "unit";

if (
  !hasRustWorkstream &&
  !hasJavaScriptWorkstream &&
  scriptTests.length === 0
) {
  printNotApplicable();
} else {
  let testsPassed = true;
  let coveragePolicy;

  if (
    testMode === "coverage" &&
    (hasReactNativeWorkstream || hasRustWorkstream)
  ) {
    try {
      coveragePolicy = loadCoveragePolicy();
    } catch (error) {
      console.error(error instanceof Error ? error.message : String(error));
      process.exitCode = 1;
      testsPassed = false;
    }
  }

  if (scriptTests.length > 0) {
    testsPassed = runCommand("node", ["--test", ...scriptTests]) && testsPassed;
  }

  if (hasReactNativeWorkstream && testsPassed) {
    if (!existsSync(`${reactNativePackage}/package.json`)) {
      console.error(
        `required configuration not found: ${reactNativePackage}/package.json`,
      );
      process.exitCode = 1;
      testsPassed = false;
    } else {
      testsPassed =
        runCommand("npm", ["test", "--prefix", reactNativePackage]) &&
        testsPassed;
    }
  }

  if (hasExpoExample && testsPassed) {
    if (!existsSync(`${expoExample}/package.json`)) {
      console.error(
        `required configuration not found: ${expoExample}/package.json`,
      );
      process.exitCode = 1;
      testsPassed = false;
    } else {
      testsPassed =
        runCommand("npm", [
          "run",
          "build",
          "--workspace",
          "@1am/midnight-mobile",
        ]) && testsPassed;
      testsPassed =
        runCommand("npm", ["test", "--prefix", expoExample]) && testsPassed;
    }
  }

  if (
    testMode === "coverage" &&
    hasReactNativeWorkstream &&
    testsPassed &&
    coveragePolicy !== undefined
  ) {
    const compiledTestsDirectory = `${reactNativePackage}/.staging-build/tests`;
    if (!existsSync(compiledTestsDirectory)) {
      console.error(
        `compiled package tests not found: ${compiledTestsDirectory}`,
      );
      process.exitCode = 1;
      testsPassed = false;
    } else {
      const compiledTests = readdirSync(compiledTestsDirectory)
        .filter((file) => file.endsWith(".test.js"))
        .sort()
        .map((file) => `${compiledTestsDirectory}/${file}`);
      if (compiledTests.length === 0) {
        console.error(
          `compiled package tests not found: ${compiledTestsDirectory}`,
        );
        process.exitCode = 1;
        testsPassed = false;
      } else {
        const minimum = coveragePolicy.typescript.minimumPercent;
        testsPassed =
          runCommand("node", [
            "--import",
            resolve(coveragePolicy.typescript.setupImport),
            "--experimental-test-coverage",
            "--test",
            `--test-coverage-include=${coveragePolicy.typescript.include}`,
            `--test-coverage-lines=${String(minimum.lines)}`,
            `--test-coverage-branches=${String(minimum.branches)}`,
            `--test-coverage-functions=${String(minimum.functions)}`,
            ...compiledTests,
          ]) && testsPassed;
      }
    }
  }

  if (hasOtherJavaScriptWorkstream) {
    if (!existsSync("jest.config.mjs")) {
      console.error("required configuration not found: jest.config.mjs");
      process.exitCode = 1;
      testsPassed = false;
    } else {
      testsPassed =
        runCommand("jest", testMode === "coverage" ? ["--coverage"] : []) &&
        testsPassed;
    }
  }

  if (hasRustWorkstream && testsPassed) {
    if (testMode === "coverage" && coveragePolicy !== undefined) {
      testsPassed =
        runCommand("cargo", [
          "llvm-cov",
          "--workspace",
          "--all-features",
          "--all-targets",
          "--fail-under-lines",
          String(coveragePolicy.rust.enforcedPercent),
          "--ignore-filename-regex",
          coveragePolicy.rust.ignoreFilenameRegex,
          "--",
          "--test-threads=1",
        ]) && testsPassed;
    } else if (testMode === "unit") {
      testsPassed =
        runCommand("cargo", [
          "test",
          "--workspace",
          "--all-features",
          "--all-targets",
          "--",
          "--test-threads=1",
        ]) && testsPassed;
    }
  }
}
