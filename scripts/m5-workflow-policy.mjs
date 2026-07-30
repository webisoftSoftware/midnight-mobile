const ACTION_SHA = /uses:\s+([^@\s]+)@([0-9a-f]{40})(?:\s|$)/gu;
const ANY_REMOTE_ACTION = /uses:\s+([^./][^@\s]*)@([^\s]+)/gu;

function requireText(errors, contents, value, label) {
  if (!contents.includes(value)) errors.push(`${label} is required`);
}

function actionPins(errors, contents, label) {
  const pinned = new Set(
    [...contents.matchAll(ACTION_SHA)].map(
      ([, action, revision]) => `${action}@${revision}`,
    ),
  );
  for (const [, action, revision] of contents.matchAll(ANY_REMOTE_ACTION)) {
    if (!pinned.has(`${action}@${revision}`)) {
      errors.push(`${label} action must use a 40-character commit: ${action}`);
    }
  }
}

function commonWorkflow(errors, contents, label) {
  requireText(errors, contents, "permissions:", `${label} permissions`);
  requireText(errors, contents, "timeout-minutes:", `${label} timeout`);
  if (contents.includes("continue-on-error: true")) {
    errors.push(`${label} must not tolerate failed steps`);
  }
  if (contents.includes("-latest")) {
    errors.push(`${label} runner images must be pinned`);
  }
  actionPins(errors, contents, label);
}

function requireTokens(errors, contents, label, tokens) {
  for (const token of tokens) {
    requireText(errors, contents, token, `${label} ${token}`);
  }
}

function validateQualityWorkflow(errors, contents) {
  commonWorkflow(errors, contents, "quality workflow");
  requireTokens(errors, contents, "quality workflow", [
    "pull_request:",
    "push:",
    "branches:",
    "- main",
    "workflow_dispatch:",
    "runs-on: macos-15",
    "npm run quality",
    "uses: ./.github/actions/setup-native",
  ]);
}

function validateDependencyWorkflow(errors, contents) {
  commonWorkflow(errors, contents, "dependency workflow");
  requireTokens(errors, contents, "dependency workflow", [
    "pull_request:",
    "push:",
    "workflow_dispatch:",
    "npm run check:security -- --source-only",
    "rustup toolchain install 1.97.1 --profile minimal",
    "cargo fetch --locked",
    "actions/setup-go@924ae3a1cded613372ab5595356fb5720e22ba16",
    "go-version: 1.26.2",
    "osv-scanner@v2.3.8",
    "OSV_SCANNER_LOCAL_DB_CACHE_DIRECTORY:",
    "osv-scanner scan source --offline --download-offline-databases",
    "artifacts/security/osv-download-results.json",
    "osv-npm-database-sha256.txt",
    "osv-scanner scan source --offline --no-resolve --format=json",
    "check-osv-findings.mjs",
    "cargo install cargo-audit --version 0.22.2 --locked",
    "cargo audit --json --deny warnings",
    "artifacts/security/cargo-audit.json",
  ]);
  if (
    contents.includes("actions/dependency-review-action") ||
    contents.includes("npm audit") ||
    contents.includes("api.osv.dev") ||
    contents.includes("osv.dev/v1") ||
    contents.includes("--offline-vulnerabilities")
  ) {
    errors.push(
      "dependency workflow must not transmit the private dependency graph",
    );
  }
}

function validateReleaseWorkflow(errors, contents) {
  commonWorkflow(errors, contents, "release workflow");
  requireTokens(errors, contents, "release workflow", [
    "DEVELOPER_DIR: /Applications/Xcode_16.4.app/Contents/Developer",
    "tags:",
    '- "v*"',
    "environment: alpha-release",
    "npm run quality",
    "prepare-release-source.mjs",
    "npm run release:evidence",
    "npm run release:verify",
    "actions/attest@f7c74d28b9d84cb8768d0b8ca14a4bac6ef463e6",
    "artifact-metadata: write",
    "sbom-path: artifacts/release/sbom.cdx.json",
    "--require-distribution-decision",
    "npm publish artifacts/native/npm/*.tgz --access public --provenance",
    'gh release create "$GITHUB_REF_NAME"',
  ]);
  const approval = contents.indexOf("--require-distribution-decision");
  const publication = contents.indexOf("npm publish");
  if (approval === -1 || publication === -1 || approval > publication) {
    errors.push("distribution approval must precede npm publication");
  }
}

function validateNativeSetup(errors, contents) {
  actionPins(errors, contents, "native setup action");
  requireTokens(errors, contents, "native setup action", [
    "node-version: 22",
    "java-version: 17.0.19+10",
    "npm@11.12.1",
    "toolchain install 1.97.1",
    "cargo-llvm-cov --version 0.8.6",
    "cocoapods --version 1.16.2",
    '"ndk;27.1.12297006"',
    '"platforms;android-24"',
    '"Xcode 16.4"',
    '"Temurin-17.0.19+10"',
  ]);
}

export function validateM5Workflows(inputs) {
  const errors = [];
  validateQualityWorkflow(errors, inputs.quality);
  validateDependencyWorkflow(errors, inputs.dependencySecurity);
  validateReleaseWorkflow(errors, inputs.release);
  validateNativeSetup(errors, inputs.setupNative);
  return errors;
}
