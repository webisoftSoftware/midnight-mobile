import { existsSync } from "node:fs";

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

if (!hasRustWorkstream && !hasJavaScriptWorkstream) {
  printNotApplicable();
} else {
  let testsPassed = true;

  if (hasJavaScriptWorkstream) {
    if (!existsSync("jest.config.mjs")) {
      console.error("required configuration not found: jest.config.mjs");
      process.exitCode = 1;
      testsPassed = false;
    } else {
      testsPassed = runCommand("jest", ["--coverage"]) && testsPassed;
    }
  }

  if (hasRustWorkstream && testsPassed) {
    const coverageToolExists = runCommand("cargo", ["llvm-cov", "--version"]);
    if (coverageToolExists) {
      runCommand("cargo", [
        "llvm-cov",
        "--workspace",
        "--all-features",
        "--all-targets",
        "--fail-under-lines",
        "80",
      ]);
    }
  }
}
