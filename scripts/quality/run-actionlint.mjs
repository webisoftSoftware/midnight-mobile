import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

import { acceptedCurrentPermission } from "./actionlint-policy.mjs";
import { listRepositoryFiles, printNotApplicable } from "./quality-utils.mjs";

const workflowPattern = /^\.github\/workflows\/.+\.ya?ml$/u;
const require = createRequire(import.meta.url);
const { getLintLog, runLint } = require("@tktco/node-actionlint");
const workflowFiles = listRepositoryFiles().filter((file) =>
  workflowPattern.test(file),
);

if (workflowFiles.length === 0) {
  printNotApplicable();
} else {
  console.log(`Checking ${String(workflowFiles.length)} workflow file(s)...`);
  const failures = [];
  for (const path of workflowFiles) {
    const data = readFileSync(path, "utf8");
    const results = await runLint(data, path);
    failures.push(
      ...results
        .filter((result) => !acceptedCurrentPermission(path, data, result))
        .map((result) => ({ ...result, path, data })),
    );
  }
  const output = getLintLog(failures);
  if (output.length > 0) {
    console.error(output);
    process.exitCode = 1;
  } else {
    console.log("✓ All workflow files passed lint checks");
  }
}
