import { createHash } from "node:crypto";
import { existsSync, readFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { validateArtifactManifest } from "./release-verification.mjs";

const REPOSITORY_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

function fail(message) {
  throw new Error(`release evidence gate: ${message}`);
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

export function verifyReleaseEvidence() {
  const manifestPath = join(
    REPOSITORY_ROOT,
    "artifacts/release/SHA256SUMS.json",
  );
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  const errors = validateArtifactManifest(manifest, (path) => {
    const absolute = join(REPOSITORY_ROOT, path);
    return existsSync(absolute)
      ? { size: statSync(absolute).size, sha256: sha256(absolute) }
      : null;
  });
  if (errors.length > 0) fail(errors.sort().join("\n"));
  console.log(
    `release evidence verified: entries=${String(manifest.entries.length)}`,
  );
}

verifyReleaseEvidence();
