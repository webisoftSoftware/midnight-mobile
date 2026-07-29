import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  parseAuditedPaths,
  validateExtractionAllowlist,
} from "./check-extraction-allowlist.mjs";

const manifest = JSON.parse(
  readFileSync("scripts/extraction-allowlist-manifest.json", "utf8"),
);
const auditedPaths = parseAuditedPaths(
  readFileSync(
    "scripts/fixtures/extraction-audited-component-paths.txt",
    "utf8",
  ),
);
const document = readFileSync("docs/EXTRACTION_ALLOWLIST.md", "utf8");

function inputs() {
  return {
    manifest: structuredClone(manifest),
    auditedPaths: [...auditedPaths],
    document,
  };
}

function includesError(errors, fragment) {
  assert.ok(
    errors.some((error) => error.includes(fragment)),
    `expected an error containing ${JSON.stringify(fragment)}:\n${errors.join("\n")}`,
  );
}

test("accepts the audited 110-path partition", () => {
  assert.deepEqual(validateExtractionAllowlist(inputs()), []);
});

test("rejects the wrong assessed authority", () => {
  const value = inputs();
  value.manifest.authority.assessedCommit = "0".repeat(40);
  const errors = validateExtractionAllowlist(value);
  includesError(errors, "authority.assessedCommit");
});

test("rejects candidate and exclusion overlap", () => {
  const value = inputs();
  value.manifest.exactExclusions[0] = value.manifest.candidates[0];
  const errors = validateExtractionAllowlist(value);
  includesError(errors, "appears in both candidates and exactExclusions");
});

test("rejects uncovered and unaudited paths", () => {
  const value = inputs();
  const replaced = value.manifest.candidates[0];
  const unaudited =
    "apps/mobile/modules/expo-midnight-native/unreviewed-source.ts";
  value.manifest.candidates[0] = unaudited;
  value.manifest.candidates.sort();
  const errors = validateExtractionAllowlist(value);
  includesError(errors, `audited path is not classified: ${replaced}`);
  includesError(
    errors,
    `classified path is not in the audited fixture: ${unaudited}`,
  );
});

test("rejects partition count drift", () => {
  const value = inputs();
  value.manifest.candidates.pop();
  const errors = validateExtractionAllowlist(value);
  includesError(errors, "candidates count must be 69, received 68");
  includesError(errors, "partition total must be 110, received 109");
});

test("rejects duplicates and paths outside the assessed component", () => {
  const duplicate = inputs();
  duplicate.manifest.candidates[1] = duplicate.manifest.candidates[0];
  includesError(
    validateExtractionAllowlist(duplicate),
    "candidates contains duplicate path",
  );

  const escaped = inputs();
  escaped.manifest.candidates[0] = "../one-am-wallet/private.ts";
  includesError(
    validateExtractionAllowlist(escaped),
    "must remain under apps/mobile/modules/expo-midnight-native/",
  );
});

test("rejects the obsolete UniFFI path and missing documentation", () => {
  const value = inputs();
  const required =
    "apps/mobile/modules/expo-midnight-native/rust/runtime/uniffi.toml";
  value.manifest.candidates = value.manifest.candidates.filter(
    (path) => path !== required,
  );
  value.manifest.candidates.push(
    "apps/mobile/modules/expo-midnight-native/rust/uniffi.toml",
  );
  value.manifest.candidates.sort();
  value.document = value.document.replace(`\`${required}\``, "");
  const errors = validateExtractionAllowlist(value);
  includesError(errors, `candidates must include ${required}`);
  includesError(
    errors,
    `allowlist document is missing audited path: ${required}`,
  );
  includesError(errors, "obsolete UniFFI path is forbidden");
});

test("rejects documentation paths assigned to the wrong partition", () => {
  const value = inputs();
  const candidate =
    "apps/mobile/modules/expo-midnight-native/expo-module.config.json";
  const excluded =
    "apps/mobile/modules/expo-midnight-native/fixtures/codec-8.1.0.json";
  const placeholder =
    "apps/mobile/modules/expo-midnight-native/__partition-swap__";
  value.document = value.document
    .replace(candidate, placeholder)
    .replace(excluded, candidate)
    .replace(placeholder, excluded);

  const errors = validateExtractionAllowlist(value);
  includesError(
    errors,
    `allowlist document candidates is missing: ${candidate}`,
  );
  includesError(
    errors,
    `allowlist document exactExclusions is missing: ${excluded}`,
  );
});

test("rejects malformed partition data without throwing", () => {
  const value = inputs();
  value.manifest.exactExclusions = "not-an-array";
  const errors = validateExtractionAllowlist(value);
  includesError(errors, "exactExclusions must be an array");
  includesError(errors, "exactExclusions count must be 39, received 0");
});
