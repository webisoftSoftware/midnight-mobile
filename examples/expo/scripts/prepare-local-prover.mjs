import { createHash } from "node:crypto";
import { cp, mkdir, readFile, rm, stat } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const exampleRoot = resolve(scriptDirectory, "..");
const repositoryRoot = resolve(exampleRoot, "../..");
const artifactRoot = resolve(
  repositoryRoot,
  "target/android-prover-spike/artifacts",
);
const manifestPath = resolve(artifactRoot, "artifact-manifest.json");
const outputRoot = resolve(exampleRoot, "assets/local-prover");

const requiredArtifacts = [
  "bls_midnight_2p15",
  "zswap/9/spend.prover",
  "zswap/9/spend.verifier",
  "zswap/9/spend.bzkir",
];

async function sha256(path) {
  return createHash("sha256")
    .update(await readFile(path))
    .digest("hex");
}

async function verifiedFile(path, expected) {
  const metadata = await stat(path);
  if (metadata.size !== expected.size) return false;
  return (await sha256(path)) === expected.sha256;
}

const manifest = await readFile(manifestPath, "utf8").catch(() => {
  throw new Error(
    "local prover artifacts are not staged; run node tools/android-prover-spike/scripts/prepare-artifacts.mjs first",
  );
});
const parsed = JSON.parse(manifest);
const byPath = new Map(
  parsed.artifacts.map((artifact) => [artifact.path, artifact]),
);

for (const path of requiredArtifacts) {
  const expected = byPath.get(path);
  const source = resolve(artifactRoot, path);
  if (expected === undefined || !(await verifiedFile(source, expected))) {
    throw new Error(
      `verified local prover artifact is missing or changed: ${path}`,
    );
  }
}

await rm(outputRoot, { recursive: true, force: true });
await mkdir(outputRoot, { recursive: true });
for (const path of requiredArtifacts) {
  await mkdir(resolve(outputRoot, dirname(path)), { recursive: true });
  await cp(resolve(artifactRoot, path), resolve(outputRoot, path));
}

console.log(
  `staged ${String(requiredArtifacts.length)} verified local prover artifacts for the Expo app`,
);
