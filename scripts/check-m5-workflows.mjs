import { readFileSync } from "node:fs";

import { validateM5Workflows } from "./m5-workflow-policy.mjs";

function read(path) {
  return readFileSync(path, "utf8");
}

const errors = validateM5Workflows({
  quality: read(".github/workflows/quality.yml"),
  fullQuality: read(".github/workflows/full-quality.yml"),
  dependencySecurity: read(".github/workflows/dependency-security.yml"),
  release: read(".github/workflows/release.yml"),
  setupNative: read(".github/actions/setup-native/action.yml"),
});

if (errors.length > 0) {
  throw new Error(`M5 workflow gate:\n${errors.sort().join("\n")}`);
}
console.log(
  "M5 workflow gate passed: lightweight PR quality, manual full qualification, portable dependency security, and deferred release automation",
);
