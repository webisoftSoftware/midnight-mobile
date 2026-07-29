import { existsSync, readdirSync } from "node:fs";

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
const hasReactNativeWorkstream = files.some((file) =>
  file.startsWith(`${reactNativePackage}/`),
);
const hasOtherJavaScriptWorkstream = files.some(
  (file) =>
    /^(?:packages|examples)\/.+\.(?:js|jsx|ts|tsx)$/u.test(file) &&
    !file.startsWith(`${reactNativePackage}/`) &&
    !file.endsWith(".d.ts"),
);
const scriptTests = files
  .filter((file) => /^scripts\/.+\.test\.mjs$/u.test(file))
  .sort();

if (
  !hasRustWorkstream &&
  !hasJavaScriptWorkstream &&
  scriptTests.length === 0
) {
  printNotApplicable();
} else {
  let testsPassed = true;
  let coveragePolicy;

  if (hasReactNativeWorkstream || hasRustWorkstream) {
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

  if (hasReactNativeWorkstream && testsPassed && coveragePolicy !== undefined) {
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
        const baseline = coveragePolicy.typescript.measuredPercent;
        testsPassed =
          runCommand("node", [
            "--experimental-test-coverage",
            "--test",
            `--test-coverage-lines=${String(baseline.lines)}`,
            `--test-coverage-branches=${String(baseline.branches)}`,
            `--test-coverage-functions=${String(baseline.functions)}`,
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
      testsPassed = runCommand("jest", ["--coverage"]) && testsPassed;
    }
  }

  if (hasRustWorkstream && testsPassed && coveragePolicy !== undefined) {
    testsPassed =
      runCommand("cargo", [
        "llvm-cov",
        "--workspace",
        "--all-features",
        "--all-targets",
        "--fail-under-lines",
        String(coveragePolicy.rust.measuredPercent),
        "--",
        "--test-threads=1",
      ]) && testsPassed;
  }
}
