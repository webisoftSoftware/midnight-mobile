import assert from "node:assert/strict";
import test from "node:test";

import { validateArtifactManifest } from "./release-verification.mjs";

const digest = "a".repeat(64);

function manifest(path = "artifacts/native/package.tgz") {
  return {
    schemaVersion: 1,
    algorithm: "SHA-256",
    entries: [{ path, sha256: digest, size: 10 }],
  };
}

await test("release verification accepts exact artifact bytes", () => {
  assert.deepEqual(
    validateArtifactManifest(manifest(), () => ({
      sha256: digest,
      size: 10,
    })),
    [],
  );
});

await test("release verification rejects unsafe, missing, and changed artifacts", () => {
  assert.ok(
    validateArtifactManifest(manifest("../secret"), () => null).some((error) =>
      error.includes("unsafe artifact path"),
    ),
  );
  assert.ok(
    validateArtifactManifest(manifest(), () => null).some((error) =>
      error.includes("artifact is missing"),
    ),
  );
  const errors = validateArtifactManifest(manifest(), () => ({
    sha256: "b".repeat(64),
    size: 11,
  }));
  assert.ok(errors.some((error) => error.includes("size mismatch")));
  assert.ok(errors.some((error) => error.includes("checksum mismatch")));
});
