import assert from "node:assert/strict";
import test from "node:test";

import { validateM5Workflows } from "./m5-workflow-policy.mjs";

function fixture() {
  const action = "actions/example@" + "a".repeat(40);
  return {
    quality: `pull_request:
push:
branches:
- main
workflow_dispatch:
permissions:
timeout-minutes:
runs-on: macos-15
npm run quality
uses: ./.github/actions/setup-native
uses: ${action}`,
    dependencySecurity: `pull_request:
push:
workflow_dispatch:
permissions:
timeout-minutes:
npm run check:security -- --source-only
rustup toolchain install 1.97.1 --profile minimal
cargo fetch --locked
uses: actions/setup-go@924ae3a1cded613372ab5595356fb5720e22ba16
go-version: 1.26.2
osv-scanner@v2.3.8
OSV_SCANNER_LOCAL_DB_CACHE_DIRECTORY:
osv-scanner scan source --offline --download-offline-databases
artifacts/security/osv-download-results.json
osv-npm-database-sha256.txt
osv-scanner scan source --offline --no-resolve --format=json
check-osv-findings.mjs
cargo install cargo-audit --version 0.22.2 --locked
cargo audit --json --deny warnings
artifacts/security/cargo-audit.json`,
    release: `permissions:
timeout-minutes:
DEVELOPER_DIR: /Applications/Xcode_16.4.app/Contents/Developer
tags:
- "v*"
environment: alpha-release
npm run quality
prepare-release-source.mjs
npm run release:evidence
npm run release:verify
uses: actions/attest@f7c74d28b9d84cb8768d0b8ca14a4bac6ef463e6
artifact-metadata: write
sbom-path: artifacts/release/sbom.cdx.json
--require-distribution-decision
npm publish artifacts/native/npm/*.tgz --access public --provenance
gh release create "$GITHUB_REF_NAME"`,
    setupNative: `node-version: 22
java-version: 17.0.19+10
npm@11.12.1
toolchain install 1.97.1
cargo-llvm-cov --version 0.8.6
uses: android-actions/setup-android@9fc6c4e9069bf8d3d10b2204b1fb8f6ef7065407
cmdline-tools-version: "14742923"
gem install --user-install cocoapods --version 1.16.2
cocoapods --version 1.16.2
"ndk;27.1.12297006"
"platforms;android-24"
"Xcode 16.4"
"Temurin-17.0.19+10"`,
  };
}

await test("complete workflows satisfy the M5 contract", () => {
  assert.deepEqual(validateM5Workflows(fixture()), []);
});

await test("unpinned actions and premature publication fail", () => {
  const inputs = fixture();
  inputs.quality = inputs.quality.replace(
    `actions/example@${"a".repeat(40)}`,
    "actions/example@main",
  );
  inputs.release = inputs.release.replace(
    "--require-distribution-decision",
    "distribution check missing",
  );
  const errors = validateM5Workflows(inputs);
  assert.ok(errors.some((error) => error.includes("40-character commit")));
  assert.ok(errors.some((error) => error.includes("approval must precede")));
});

await test("dependency security rejects graph egress and partial offline mode", () => {
  const inputs = fixture();
  inputs.dependencySecurity +=
    "\nnpm audit --json\nosv-scanner --offline-vulnerabilities";
  assert.ok(
    validateM5Workflows(inputs).some((error) =>
      error.includes("must not transmit"),
    ),
  );
});
