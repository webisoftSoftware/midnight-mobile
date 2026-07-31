import {
  MidnightRuntimeError,
  type MidnightFetch,
  type MidnightFetchRequest,
  type MidnightFetchResponse,
} from "@1am/midnight-mobile";

/**
 * Translates the runtime's `node` submission effect into Substrate JSON-RPC.
 *
 * The runtime emits a `network` step with `endpointRole: "node"` whose body is
 * the canonical ledger transaction — `tagged_serialize(Transaction)`, the same
 * bytes it validated. A Midnight node does not accept those bytes directly. It
 * accepts a SCALE-encoded unsigned extrinsic calling
 * `pallet_midnight::send_mn_transaction`, hex-encoded, through
 * `author_submitExtrinsic`.
 *
 * The framing below was read off a real on-chain extrinsic rather than inferred
 * from the metadata, which is why the indices are trusted (block 216733 of
 * chain "Midnight Preview", node 1.0.1-5edf8ddd):
 *
 *   compact(len)          outer length prefix for the RPC's `Bytes` parameter
 *   0x04                  bare (unsigned) extrinsic, format version 4
 *   0x05 0x00             pallet 5 call 0 = Midnight::send_mn_transaction
 *   compact(len) + bytes  the transaction as a SCALE `Vec<u8>`
 *
 * The payload of that extrinsic began `midnight:transaction[v9](signature[v1]…`,
 * which is exactly the tag our pinned ledger (8.1.0, rev 02716c2c) emits, so the
 * transaction bytes need no re-encoding — only wrapping.
 *
 * Pallet and call indices are numbered by runtime construction and a runtime
 * upgrade can renumber them, so they are overridable. If submissions start
 * failing verification after a node upgrade, that is the first thing to check.
 */

const EXTRINSIC_FORMAT_BARE = 0x04;
const DEFAULT_PALLET_INDEX = 5;
const DEFAULT_CALL_INDEX = 0;

export interface NodeCallIndices {
  readonly palletIndex: number;
  readonly callIndex: number;
}

const DEFAULT_INDICES: NodeCallIndices = {
  palletIndex: DEFAULT_PALLET_INDEX,
  callIndex: DEFAULT_CALL_INDEX,
};

/** SCALE compact integer. Covers the single, two, and four byte modes. */
function encodeCompact(value: number): Uint8Array {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new MidnightRuntimeError("INVALID_ARGUMENT");
  }
  if (value < 0b1000000) return new Uint8Array([value << 2]);
  if (value < 0b100000000000000) {
    const encoded = (value << 2) | 0b01;
    return new Uint8Array([encoded & 0xff, (encoded >>> 8) & 0xff]);
  }
  if (value < 0b1000000000000000000000000000000) {
    const encoded = ((value << 2) | 0b10) >>> 0;
    return new Uint8Array([
      encoded & 0xff,
      (encoded >>> 8) & 0xff,
      (encoded >>> 16) & 0xff,
      (encoded >>> 24) & 0xff,
    ]);
  }
  // A transaction that large is rejected by the runtime long before this point.
  throw new MidnightRuntimeError("INVALID_ARGUMENT");
}

function concat(parts: readonly Uint8Array[]): Uint8Array {
  const total = parts.reduce((sum, part) => sum + part.length, 0);
  const joined = new Uint8Array(total);
  let offset = 0;
  for (const part of parts) {
    joined.set(part, offset);
    offset += part.length;
  }
  return joined;
}

/** Wraps canonical transaction bytes in a bare `send_mn_transaction` extrinsic. */
export function encodeSubmitExtrinsic(
  transaction: Uint8Array,
  indices: NodeCallIndices = DEFAULT_INDICES,
): Uint8Array {
  if (transaction.length === 0) {
    throw new MidnightRuntimeError("INVALID_ARGUMENT");
  }
  const call = concat([
    new Uint8Array([
      EXTRINSIC_FORMAT_BARE,
      indices.palletIndex,
      indices.callIndex,
    ]),
    encodeCompact(transaction.length),
    transaction,
  ]);
  return concat([encodeCompact(call.length), call]);
}

function toHex(bytes: Uint8Array): string {
  let hex = "0x";
  for (const byte of bytes) {
    hex += byte.toString(16).padStart(2, "0");
  }
  return hex;
}

/**
 * Substrate author-RPC error codes that mean the transaction is definitively
 * invalid: it will never be included, so reporting `rejected` is honest.
 *
 * 1001 bad format, 1002 verification failure, 1010 invalid transaction.
 */
const DEFINITELY_REJECTED = new Set([1001, 1002, 1010]);

/** 1013 already imported: the transaction is in the pool, so it was accepted. */
const ALREADY_IMPORTED = 1013;

/**
 * Maps a JSON-RPC envelope onto the HTTP status the transport classifies with.
 *
 * This mapping is the reason the adapter cannot just forward the response.
 * JSON-RPC answers with HTTP 200 even when the call failed, and the transport
 * derives its outcome from the status alone — so a rejected extrinsic would be
 * reported to the runtime as an accepted submission.
 *
 * Anything not positively identified becomes 502, which the transport turns into
 * `statusUnknown`. That is the safe direction: the runtime keeps the submission
 * resumable instead of recording a verdict it cannot support.
 */
export function classifyNodeResponse(body: Uint8Array): number {
  let envelope: unknown;
  try {
    envelope = JSON.parse(new TextDecoder().decode(body));
  } catch {
    return 502;
  }
  if (typeof envelope !== "object" || envelope === null) return 502;
  const { result, error } = envelope as {
    readonly result?: unknown;
    readonly error?: unknown;
  };
  if (typeof result === "string") return 200;
  if (typeof error !== "object" || error === null) return 502;
  const code = (error as { readonly code?: unknown }).code;
  if (typeof code !== "number") return 502;
  if (code === ALREADY_IMPORTED) return 200;
  return DEFINITELY_REJECTED.has(code) ? 400 : 502;
}

function staticResponse(
  status: number,
  body: Uint8Array,
): MidnightFetchResponse {
  return {
    status,
    arrayBuffer(): Promise<ArrayBuffer> {
      const copy = new Uint8Array(new ArrayBuffer(body.length));
      copy.set(body);
      return Promise.resolve(copy.buffer);
    },
  };
}

function jsonRequest(
  request: MidnightFetchRequest,
  method: string,
  params: readonly unknown[],
): MidnightFetchRequest {
  return {
    method: "POST",
    headers: { ...request.headers, "content-type": "application/json" },
    body: new TextEncoder().encode(
      JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
    ),
    signal: request.signal,
  };
}

async function submitToNode(
  inner: MidnightFetch,
  url: string,
  request: MidnightFetchRequest,
  indices: NodeCallIndices,
): Promise<MidnightFetchResponse> {
  const extrinsic = toHex(encodeSubmitExtrinsic(request.body, indices));
  const response = await inner(
    url,
    jsonRequest(request, "author_submitExtrinsic", [extrinsic]),
  );
  const body = new Uint8Array(await response.arrayBuffer());
  // An HTTP-level failure is already unambiguous; only a 2xx carries a JSON-RPC
  // envelope worth classifying.
  const status =
    response.status >= 200 && response.status < 300
      ? classifyNodeResponse(body)
      : response.status;
  return staticResponse(status, body);
}

function normalize(url: string): string {
  try {
    return new URL(url).toString();
  } catch {
    throw new MidnightRuntimeError("INVALID_ARGUMENT");
  }
}

/**
 * Wraps a transport `fetch` so requests aimed at the node URL are translated
 * into JSON-RPC and everything else passes through untouched.
 *
 * Branching on the URL rather than the role is forced by the contract:
 * `MidnightFetch` receives a URL and a body, not the originating step. The
 * transport normalizes its configured URLs through `new URL(...)` before calling
 * fetch, so both sides are normalized here to compare the same form.
 */
export function createNodeAwareFetch(
  inner: MidnightFetch,
  nodeUrl: string,
  indices: NodeCallIndices = DEFAULT_INDICES,
): MidnightFetch {
  const target = normalize(nodeUrl);
  return (url, request) =>
    normalize(url) === target
      ? submitToNode(inner, url, request, indices)
      : inner(url, request);
}

/**
 * Calls a read-only node method, for reachability checks.
 *
 * Returns the `result` field as text. Unlike an empty POST, this proves the
 * endpoint is a working JSON-RPC node rather than merely a reachable socket.
 */
export async function callNodeMethod(
  inner: MidnightFetch,
  nodeUrl: string,
  method: string,
  signal: AbortSignal,
): Promise<string> {
  const response = await inner(nodeUrl, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: new TextEncoder().encode(
      JSON.stringify({ jsonrpc: "2.0", id: 1, method, params: [] }),
    ),
    signal,
  });
  const text = new TextDecoder().decode(
    new Uint8Array(await response.arrayBuffer()),
  );
  const envelope: unknown = JSON.parse(text);
  const result =
    typeof envelope === "object" && envelope !== null
      ? (envelope as { readonly result?: unknown }).result
      : undefined;
  if (result === undefined) throw new MidnightRuntimeError("TRANSPORT_ERROR");
  return typeof result === "string" ? result : JSON.stringify(result);
}
