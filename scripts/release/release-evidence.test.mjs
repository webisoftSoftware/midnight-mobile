import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  fingerprintEvidence,
  renderReleaseEvidence,
} from "./release-evidence.mjs";

const config = JSON.parse(
  readFileSync(new URL("./release-config.json", import.meta.url), "utf8"),
);

function fixture() {
  return {
    config,
    npmLockfile: {
      packages: {
        "node_modules/example": {
          version: "1.0.0",
          license: "MIT",
          resolved: "https://registry.npmjs.org/example/-/example-1.0.0.tgz",
          integrity: "sha512-example",
        },
      },
    },
    cargoMetadata: {
      packages: [
        {
          name: "runtime",
          version: "1.0.0",
          license: "Apache-2.0",
          source: null,
        },
      ],
    },
    artifactEntries: [
      { path: "artifact.so", sha256: "a".repeat(64), size: 10 },
    ],
    context: {
      gitCommit: "b".repeat(40),
      commitTimestamp: "2026-07-29T00:00:00Z",
      sourceTreeSha256: "c".repeat(64),
    },
  };
}

await test("release evidence is deterministic and inventories both graphs", () => {
  const first = renderReleaseEvidence(fixture());
  const second = renderReleaseEvidence(fixture());
  assert.deepEqual(first, second);
  assert.equal(fingerprintEvidence(first), fingerprintEvidence(second));
  const sbom = JSON.parse(first["sbom.cdx.json"]);
  assert.equal(sbom.bomFormat, "CycloneDX");
  assert.equal(sbom.components.length, 2);
  const licenses = JSON.parse(first["licenses.json"]);
  assert.equal(licenses.projectLicense, "MIT");
  assert.equal(licenses.componentCount, 2);
  const checksums = JSON.parse(first["SHA256SUMS.json"]);
  assert.equal(checksums.entries.length, 4);
});

await test("provenance binds artifact subjects and exact source material", () => {
  const evidence = renderReleaseEvidence(fixture());
  const provenance = JSON.parse(evidence["provenance.json"]);
  assert.equal(provenance.subject[0].name, "artifact.so");
  assert.equal(
    provenance.predicate.buildDefinition.resolvedDependencies[0].digest
      .gitCommit,
    "b".repeat(40),
  );
  assert.equal(
    provenance.predicate.buildDefinition.resolvedDependencies[1].digest
      .gitCommit,
    config.compatibility.midnightLedgerRevision,
  );
});
