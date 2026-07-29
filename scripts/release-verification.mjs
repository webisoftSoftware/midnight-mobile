import { posix } from "node:path";

function validEntry(entry) {
  return (
    typeof entry === "object" &&
    entry !== null &&
    typeof entry.path === "string" &&
    typeof entry.sha256 === "string" &&
    /^[0-9a-f]{64}$/u.test(entry.sha256) &&
    Number.isSafeInteger(entry.size) &&
    entry.size >= 0
  );
}

function safeArtifactPath(entry, errors) {
  if (!validEntry(entry)) {
    errors.push("checksum manifest entry is invalid");
    return null;
  }
  const path = posix.normalize(entry.path);
  if (
    path !== entry.path ||
    path.startsWith("../") ||
    path.startsWith("/") ||
    !path.startsWith("artifacts/")
  ) {
    errors.push(`unsafe artifact path: ${entry.path}`);
    return null;
  }
  return path;
}

function verifyArtifact(path, entry, artifact, errors) {
  const actual = artifact(path);
  if (actual === null) {
    errors.push(`artifact is missing: ${path}`);
    return;
  }
  if (actual.size !== entry.size) {
    errors.push(`artifact size mismatch: ${path}`);
  }
  if (actual.sha256 !== entry.sha256) {
    errors.push(`artifact checksum mismatch: ${path}`);
  }
}

export function validateArtifactManifest(manifest, artifact) {
  const errors = [];
  if (
    typeof manifest !== "object" ||
    manifest === null ||
    manifest.schemaVersion !== 1 ||
    manifest.algorithm !== "SHA-256"
  ) {
    return ["checksum manifest header is invalid"];
  }
  if (!Array.isArray(manifest.entries) || manifest.entries.length === 0) {
    return ["checksum manifest has no entries"];
  }
  const names = new Set();
  let previous = "";
  for (const entry of manifest.entries) {
    const path = safeArtifactPath(entry, errors);
    if (path === null) continue;
    if (names.has(path)) errors.push(`duplicate artifact path: ${path}`);
    names.add(path);
    if (previous.localeCompare(path) > 0) {
      errors.push("artifact entries must be sorted");
    }
    previous = path;
    verifyArtifact(path, entry, artifact, errors);
  }
  return errors;
}
