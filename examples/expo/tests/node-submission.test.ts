import assert from "node:assert/strict";
import test from "node:test";

import {
  MidnightRuntimeError,
  type MidnightFetch,
  type MidnightFetchRequest,
} from "@1am/midnight-mobile";

import {
  classifyNodeResponse,
  createNodeAwareFetch,
  encodeSubmitExtrinsic,
} from "../src/node-submission";

function hex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join(
    "",
  );
}

await test("submit extrinsic reproduces the observed on-chain framing", () => {
  // Block 216733 of chain "Midnight Preview" carried a 3820-byte transaction in
  // an extrinsic beginning c53b040500b13b: outer compact(3825), format 0x04,
  // pallet 5, call 0, inner compact(3820). Reproducing that prefix and total
  // length exactly is what proves the framing, so both are pinned here rather
  // than described in prose.
  const encoded = encodeSubmitExtrinsic(new Uint8Array(3820).fill(0xab));
  assert.equal(hex(encoded.subarray(0, 7)), "c53b040500b13b");
  assert.equal(encoded.length, 3827);
  assert.equal(encoded.at(-1), 0xab);
});

await test("compact length prefixes cover every SCALE mode boundary", () => {
  // A wrong prefix at a mode boundary decodes on the node as a malformed
  // extrinsic, which surfaces as a verification failure rather than as an
  // encoding bug — so each boundary is pinned to a known-correct vector.
  const cases: readonly (readonly [number, string])[] = [
    [1, "140405000400"],
    [63, "0d01040500fc"],
    [64, "1501040500" + "0101"],
    [16383, "12000100040500" + "fdff"],
  ];
  for (const [length, prefix] of cases) {
    const encoded = hex(encodeSubmitExtrinsic(new Uint8Array(length)));
    assert.equal(
      encoded.startsWith(prefix),
      true,
      `length ${String(length)} produced ${encoded.slice(0, 20)}`,
    );
  }
});

await test("the transaction body is embedded verbatim", () => {
  assert.equal(
    hex(encodeSubmitExtrinsic(new Uint8Array([7, 8, 9]))),
    "1c0405000c070809",
  );
});

await test("an empty transaction is rejected before it reaches the node", () => {
  assert.throws(
    () => encodeSubmitExtrinsic(new Uint8Array(0)),
    (error: unknown) =>
      error instanceof MidnightRuntimeError &&
      error.code === "INVALID_ARGUMENT",
  );
});

await test("pallet and call indices are overridable for a renumbered runtime", () => {
  const encoded = encodeSubmitExtrinsic(new Uint8Array([1]), {
    palletIndex: 9,
    callIndex: 2,
  });
  assert.equal(hex(encoded), "140409020401");
});

function errorEnvelope(code: number): Uint8Array {
  return new TextEncoder().encode(
    JSON.stringify({ jsonrpc: "2.0", id: 1, error: { code, message: "x" } }),
  );
}

await test("a JSON-RPC result is what marks a submission accepted", () => {
  assert.equal(
    classifyNodeResponse(
      new TextEncoder().encode('{"jsonrpc":"2.0","id":1,"result":"0xabc"}'),
    ),
    200,
  );
});

await test("only definitively invalid transactions are reported as rejected", () => {
  // Bad format, verification failure, and invalid transaction will never be
  // included, so a definite verdict is honest.
  for (const code of [1001, 1002, 1010]) {
    assert.equal(classifyNodeResponse(errorEnvelope(code)), 400);
  }
  // Already imported means the transaction is in the pool: it was accepted.
  assert.equal(classifyNodeResponse(errorEnvelope(1013)), 200);
  // Everything else must degrade to statusUnknown so the runtime keeps the
  // submission resumable instead of recording a verdict it cannot support.
  for (const code of [1011, 1012, 1014, 1016, -32603]) {
    assert.equal(classifyNodeResponse(errorEnvelope(code)), 502);
  }
});

await test("an unparseable or empty body is statusUnknown, never accepted", () => {
  for (const body of ["<html>", "null", "{}", '{"result":42}', ""]) {
    assert.equal(classifyNodeResponse(new TextEncoder().encode(body)), 502);
  }
});

const NODE_URL = "https://node.example/";
const PROOF_URL = "https://proof.example/";

interface SeenRequest {
  readonly url: string;
  readonly contentType: string | undefined;
  readonly body: Uint8Array;
}

function recordingFetch(
  status: number,
  body: string,
): { readonly fetch: MidnightFetch; readonly seen: readonly SeenRequest[] } {
  const seen: SeenRequest[] = [];
  const fetch: MidnightFetch = (url, request) => {
    seen.push({
      url,
      contentType: request.headers["content-type"],
      body: request.body,
    });
    const bytes = new TextEncoder().encode(body);
    return Promise.resolve({
      status,
      arrayBuffer: () =>
        Promise.resolve(
          bytes.buffer.slice(
            bytes.byteOffset,
            bytes.byteOffset + bytes.byteLength,
          ),
        ),
    });
  };
  return { fetch, seen };
}

function octetRequest(body: Uint8Array): MidnightFetchRequest {
  return {
    method: "POST",
    headers: { "content-type": "application/octet-stream" },
    body,
    signal: new AbortController().signal,
  };
}

await test("only node requests are translated; others pass through", async () => {
  const { fetch, seen } = recordingFetch(200, '{"result":"0xhash"}');
  const wrapped = createNodeAwareFetch(fetch, NODE_URL);
  const request = octetRequest(new Uint8Array([7, 8, 9]));

  await wrapped(PROOF_URL, request);
  assert.equal(seen[0]?.contentType, "application/octet-stream");
  assert.deepEqual(seen[0].body, new Uint8Array([7, 8, 9]));

  const response = await wrapped(NODE_URL, request);
  assert.equal(seen[1]?.contentType, "application/json");
  const sent: unknown = JSON.parse(new TextDecoder().decode(seen[1].body));
  assert.deepEqual(sent, {
    jsonrpc: "2.0",
    id: 1,
    method: "author_submitExtrinsic",
    params: ["0x1c0405000c070809"],
  });
  assert.equal(response.status, 200);
});

await test("the node URL is matched after normalization", async () => {
  const { fetch, seen } = recordingFetch(200, '{"result":"0xhash"}');
  // The transport passes its configured URL through `new URL(...)` before
  // calling fetch, so a configuration written without a trailing slash must
  // still match the normalized URL that actually arrives.
  const wrapped = createNodeAwareFetch(fetch, "https://node.example");
  await wrapped(NODE_URL, octetRequest(new Uint8Array([1])));
  assert.equal(seen[0]?.contentType, "application/json");
});

await test("a node HTTP failure keeps its own status", async () => {
  const { fetch } = recordingFetch(503, "gateway down");
  const wrapped = createNodeAwareFetch(fetch, NODE_URL);
  const response = await wrapped(NODE_URL, octetRequest(new Uint8Array([1])));
  // 503 already means statusUnknown to the transport; it must not be rewritten
  // into a definite verdict by the JSON-RPC classifier.
  assert.equal(response.status, 503);
});
