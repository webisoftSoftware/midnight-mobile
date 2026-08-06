// Turns a compiled benchmark contract into a real local-prover /prove request.
//
// Executes the circuit locally -- no network, no providers, no deployed contract -- so the
// preimage carries a genuine transcript rather than a hand-built one. That matters because
// the IR machine rejects a malformed transcript during preprocessing, before any of the
// polynomial work whose cost this harness exists to measure.
//
// The two Midnight packages imported here are not dependencies of this repository; they
// resolve from the consuming wallet checkout. See ../README.md for how to run this.
//
// Usage: node scripts/build-request.mjs <managed-directory> <output-file> [--no-inline-keys]

import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

import {
  createCircuitContext,
  createConstructorContext,
  dummyContractAddress,
  proofDataIntoSerializedPreimage,
} from "@midnight-ntwrk/compact-runtime";
import { createProvingPayload } from "@midnight-ntwrk/ledger-v8";

const [managedArgument, outputArgument] = process.argv.slice(2);
if (!managedArgument || !outputArgument) {
  throw new Error("usage: build-request.mjs <managed-directory> <output-file>");
}
const managed = resolve(managedArgument);
const output = resolve(outputArgument);

// Must match BENCH_KEY_LOCATION in the circuit-size-bench example.
const KEY_LOCATION = "bench/run";
const COIN_PUBLIC_KEY = "00".repeat(32);

// Above roughly 32 MB of prover key an inlined request exceeds the ABI's request-size cap,
// so those circuits are registry-served and the request carries only the preimage.
const inlineKeys = !process.argv.includes("--no-inline-keys");

const { Contract } = await import(
  pathToFileURL(resolve(managed, "contract/index.js")).href
);

// The witness only has to be well formed; the padding chain hashes whatever it returns.
const contract = new Contract({
  wit_seed: ({ privateState }) => [privateState, new Uint8Array(32).fill(7)],
});

const constructed = contract.initialState(
  createConstructorContext({}, COIN_PUBLIC_KEY),
);
const context = createCircuitContext(
  dummyContractAddress(),
  COIN_PUBLIC_KEY,
  constructed.currentContractState,
  constructed.currentPrivateState,
);
const { proofData } = contract.impureCircuits.run(context);

const preimage = proofDataIntoSerializedPreimage(
  proofData.input,
  proofData.output,
  proofData.publicTranscript,
  proofData.privateTranscriptOutputs,
  KEY_LOCATION,
);

const request = inlineKeys
  ? createProvingPayload(preimage, undefined, {
      proverKey: readFileSync(resolve(managed, "keys/run.prover")),
      verifierKey: readFileSync(resolve(managed, "keys/run.verifier")),
      ir: readFileSync(resolve(managed, "zkir/run.bzkir")),
    })
  : createProvingPayload(preimage, undefined);

writeFileSync(output, request);
console.log(
  JSON.stringify({
    keyLocation: KEY_LOCATION,
    inlineKeys,
    preimageBytes: preimage.length,
    requestBytes: request.length,
    output,
  }),
);
