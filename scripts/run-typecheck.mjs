import { existsSync } from "node:fs";

import {
  listRepositoryFiles,
  printNotApplicable,
  runCommand,
} from "./quality-utils.mjs";

const typedFiles = listRepositoryFiles().filter(
  (file) => file.endsWith(".ts") || file.endsWith(".tsx"),
);

if (typedFiles.length === 0) {
  printNotApplicable();
} else if (!existsSync("tsconfig.json")) {
  console.error("required configuration not found: tsconfig.json");
  process.exitCode = 1;
} else {
  runCommand("tsc", ["--noEmit", "--project", "tsconfig.json"]);
}
