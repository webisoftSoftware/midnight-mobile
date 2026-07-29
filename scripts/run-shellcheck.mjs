import { readFileSync } from "node:fs";

import {
  listRepositoryFiles,
  printNotApplicable,
  runCommand,
} from "./quality-utils.mjs";

const shellExtensionPattern = /\.(?:bash|sh|zsh)$/u;
const shellShebangPattern = /^#!.*\b(?:ba|z|k)?sh\b/u;

function isShellScript(file) {
  if (shellExtensionPattern.test(file)) {
    return true;
  }

  try {
    const firstLine = readFileSync(file, "utf8").split(/\r?\n/u, 1)[0] ?? "";
    return shellShebangPattern.test(firstLine);
  } catch {
    return false;
  }
}

const shellFiles = listRepositoryFiles().filter(isShellScript);

if (shellFiles.length === 0) {
  printNotApplicable();
} else {
  runCommand("shellcheck", ["--severity=style", ...shellFiles]);
}
