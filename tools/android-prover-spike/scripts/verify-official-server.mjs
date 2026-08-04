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
const checkRequest = readFileSync(
  resolve(targetRoot, "artifacts/check-request.bin"),
);
const outputRequest = readFileSync(
  resolve(targetRoot, "artifacts/output-request.bin"),
);
const base = endpoint.endsWith("/") ? endpoint : `${endpoint}/`;
const checkResponse = await fetch(new URL("check", base), {
  method: "POST",
  headers: { "content-type": "application/octet-stream" },
  body: checkRequest,
});
if (!checkResponse.ok) {
  throw new Error(
    `official /check returned HTTP ${String(checkResponse.status)}`,
  );
}
const checked = Buffer.from(await checkResponse.arrayBuffer());
const checkValidation = spawnSync(
  "cargo",
  [
    "run",
    "--offline",
    "--locked",
    "--quiet",
    "--package",
    "midnight-mobile-runtime",
    "--features",
    "local-prover",
    "--example",
    "validate_android_prover_check",
  ],
  { cwd: repositoryRoot, input: checked, encoding: "utf8" },
);
if (checkValidation.status !== 0) {
  throw new Error(
    `official response is not a tagged check result: ${checkValidation.stderr}`,
  );
}
const response = await fetch(new URL("prove", base), {
  method: "POST",
  headers: { "content-type": "application/octet-stream" },
  body: request,
});
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
    "midnight-mobile-runtime",
    "--features",
    "local-prover",
    "--example",
    "validate_android_prover_proof",
  ],
  { cwd: repositoryRoot, input: proof, encoding: "utf8" },
);
if (validation.status !== 0) {
  throw new Error(`official response is not tagged V2: ${validation.stderr}`);
}
const outputResponse = await fetch(new URL("prove", base), {
  method: "POST",
  headers: { "content-type": "application/octet-stream" },
  body: outputRequest,
});
if (!outputResponse.ok) {
  throw new Error(
    `official output /prove returned HTTP ${String(outputResponse.status)}`,
  );
}
const outputProof = Buffer.from(await outputResponse.arrayBuffer());
writeFileSync(
  resolve(targetRoot, "official-server-output-proof.bin"),
  outputProof,
);
const outputValidation = spawnSync(
  "cargo",
  [
    "run",
    "--offline",
    "--locked",
    "--quiet",
    "--package",
    "midnight-mobile-runtime",
    "--features",
    "local-prover",
    "--example",
    "validate_android_prover_proof",
  ],
  { cwd: repositoryRoot, input: outputProof, encoding: "utf8" },
);
if (outputValidation.status !== 0) {
  throw new Error(
    `official output response is not tagged V2: ${outputValidation.stderr}`,
  );
}
console.log(
  `official /check and both /prove requests accepted: checkBytes=${String(checked.length)} ` +
    `spendProofBytes=${String(proof.length)} outputProofBytes=${String(outputProof.length)} ` +
    `sha256=${createHash("sha256").update(proof).digest("hex")}`,
);
