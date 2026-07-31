import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = resolve(
  dirname(fileURLToPath(import.meta.url)),
  "../../..",
);
const targetRoot = resolve(repositoryRoot, "target/android-prover-spike");
const endpoint = process.argv[2] ?? process.env.MIDNIGHT_PROOF_SERVER_URL;
if (endpoint === undefined) {
  throw new Error(
    "pass the proof-server base URL or set MIDNIGHT_PROOF_SERVER_URL",
  );
}

const request = readFileSync(resolve(targetRoot, "artifacts/request.bin"));
const response = await fetch(
  new URL("prove", endpoint.endsWith("/") ? endpoint : `${endpoint}/`),
  {
    method: "POST",
    headers: { "content-type": "application/octet-stream" },
    body: request,
  },
);
if (!response.ok)
  throw new Error(`official /prove returned HTTP ${String(response.status)}`);
const proof = Buffer.from(await response.arrayBuffer());
writeFileSync(resolve(targetRoot, "official-server-proof.bin"), proof);
const validation = spawnSync(
  "cargo",
  [
    "run",
    "--offline",
    "--locked",
    "--quiet",
    "--package",
    "midnight-native-runtime",
    "--features",
    "android-prover-spike",
    "--example",
    "validate_android_prover_proof",
  ],
  { cwd: repositoryRoot, input: proof, encoding: "utf8" },
);
if (validation.status !== 0) {
  throw new Error(`official response is not tagged V2: ${validation.stderr}`);
}
console.log(
  `official /prove accepted request: bytes=${String(proof.length)} ` +
    `sha256=${createHash("sha256").update(proof).digest("hex")}`,
);
