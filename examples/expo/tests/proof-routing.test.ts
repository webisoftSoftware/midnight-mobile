import assert from "node:assert/strict";
import test from "node:test";

import type { MidnightFetch, MidnightFetchRequest } from "@1am/midnight-mobile";

import { createProofAwareFetch, proofRouteFor } from "../src/proof-routing";

// The exact tag headers proof-server 8.1.0 reports for each path.
const CHECK_TAG = "midnight:(proof-preimage-versioned,option(wrapped-ir)):";
const PROVE_TAG =
  "midnight:(proof-preimage-versioned,option(proving-data),option(fr-bls)):";

function tagged(tag: string): Uint8Array {
  const header = new TextEncoder().encode(tag);
  const body = new Uint8Array(header.length + 3);
  body.set(header);
  return body;
}

await test("each proof payload routes by its own tag header", () => {
  assert.equal(proofRouteFor(tagged(CHECK_TAG)), "check");
  assert.equal(proofRouteFor(tagged(PROVE_TAG)), "prove");
});

await test("an unrecognized payload is not routed", () => {
  // Guessing a path would produce a payload proved as the wrong shape. Passing
  // through fails at the server's header check with a message naming the tag.
  assert.equal(proofRouteFor(new Uint8Array(0)), undefined);
  assert.equal(proofRouteFor(tagged("midnight:(something-else):")), undefined);
  // A truncated tag must not match the longer route it prefixes.
  assert.equal(
    proofRouteFor(new TextEncoder().encode(PROVE_TAG.slice(0, 40))),
    undefined,
  );
});

await test("the check tag is not confused with the prove tag", () => {
  // Both begin "midnight:(proof-preimage-versioned,option(", so a prefix match
  // that stopped early would send checks to /prove.
  assert.equal(CHECK_TAG.slice(0, 42), PROVE_TAG.slice(0, 42));
  assert.notEqual(
    proofRouteFor(tagged(CHECK_TAG)),
    proofRouteFor(tagged(PROVE_TAG)),
  );
});

const PROOF_URL = "http://localhost:6300/";
const NODE_URL = "https://node.example/";

function recordingFetch(): {
  readonly fetch: MidnightFetch;
  readonly urls: readonly string[];
} {
  const urls: string[] = [];
  const fetch: MidnightFetch = (url) => {
    urls.push(url);
    return Promise.resolve({
      status: 200,
      arrayBuffer: () => Promise.resolve(new ArrayBuffer(0)),
    });
  };
  return { fetch, urls };
}

function request(body: Uint8Array): MidnightFetchRequest {
  return {
    method: "POST",
    headers: { "content-type": "application/octet-stream" },
    body,
    signal: new AbortController().signal,
  };
}

await test("proof requests are rewritten to their path", async () => {
  const { fetch, urls } = recordingFetch();
  const wrapped = createProofAwareFetch(fetch, PROOF_URL);
  await wrapped(PROOF_URL, request(tagged(CHECK_TAG)));
  await wrapped(PROOF_URL, request(tagged(PROVE_TAG)));
  assert.deepEqual(urls, [
    "http://localhost:6300/check",
    "http://localhost:6300/prove",
  ]);
});

await test("non-proof URLs are left alone", async () => {
  const { fetch, urls } = recordingFetch();
  const wrapped = createProofAwareFetch(fetch, PROOF_URL);
  await wrapped(NODE_URL, request(tagged(CHECK_TAG)));
  assert.deepEqual(urls, [NODE_URL]);
});

await test("a proof URL configured without a trailing slash still matches", async () => {
  const { fetch, urls } = recordingFetch();
  const wrapped = createProofAwareFetch(fetch, "http://localhost:6300");
  await wrapped(PROOF_URL, request(tagged(PROVE_TAG)));
  assert.deepEqual(urls, ["http://localhost:6300/prove"]);
});

await test("an adopter's own path is never overwritten", async () => {
  const { fetch, urls } = recordingFetch();
  // Configuring a path means the adopter has taken over routing — perhaps a
  // gateway that dispatches internally. Replacing it silently would break them.
  const explicit = "http://localhost:6300/proofs/v1";
  const wrapped = createProofAwareFetch(fetch, explicit);
  await wrapped(explicit, request(tagged(CHECK_TAG)));
  assert.deepEqual(urls, [explicit]);
});
