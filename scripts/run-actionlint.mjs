import {
  listRepositoryFiles,
  printNotApplicable,
  runCommand,
} from "./quality-utils.mjs";

const workflowPattern = /^\.github\/workflows\/.+\.ya?ml$/u;
const workflowFiles = listRepositoryFiles().filter((file) =>
  workflowPattern.test(file),
);

if (workflowFiles.length === 0) {
  printNotApplicable();
} else {
  runCommand("node-actionlint", workflowFiles);
}
