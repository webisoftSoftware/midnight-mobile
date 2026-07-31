import {
  MidnightRuntimeError,
  type MidnightSyncStream,
} from "@1am/midnight-mobile";

/**
 * Maps Midnight indexer GraphQL subscription data into the exact payload bytes
 * the Rust runtime accepts.
 *
 * The wire contract, read off the runtime rather than guessed:
 *
 * - `shielded` and `dust` payloads are RAW LEDGER BYTES. `apply_batch` feeds
 *   them to `tagged_deserialize_sequence::<Event>`, so the host hex-decodes the
 *   indexer's `raw` field and passes the bytes. The JSON `ShieldedWireEvent` /
 *   `DustWireEvent` shapes belong to the `shielded-wire` / `dust-wire` streams,
 *   which the public TypeScript API cannot reach: `MidnightSyncStream` is only
 *   "shielded" | "unshielded" | "dust".
 * - `unshielded` payloads are JSON of the `UnshieldedSyncUpdate` enum, tagged by
 *   a field literally named `type` — not `__typename`.
 *
 * Every one of these structs is `#[serde(deny_unknown_fields)]`. An extra field
 * passed through from GraphQL is INVALID_ARGUMENT, with no indication of which
 * field caused it, so each projection below emits exactly the accepted keys and
 * the selection sets request exactly those fields. `UnshieldedUtxo` in
 * particular has ten fields on the server and seven in `WireUtxo`.
 *
 * Offsets: subscriptions take the last-seen id, and event ids run from
 * `fromOffset + 1`. `applySyncBatch` requires `fromOffset` to equal the
 * session's current offset for the stream and stores `toOffset` as the new
 * position, so `toOffset` is the last id in the batch.
 */

const SHIELDED_SUBSCRIPTION = `subscription ZswapEvents($id: Int) {
  zswapLedgerEvents(id: $id) { id raw }
}`;

const DUST_SUBSCRIPTION = `subscription DustEvents($id: Int) {
  dustLedgerEvents(id: $id) { id raw }
}`;

// `transaction.type` feeds a `SystemTransaction` comparison in the runtime, so
// __typename is aliased to `type`. `block` and `transactionResult` are optional
// in WireTransaction and only exist on some Transaction variants, so they are
// requested through inline fragments.
const UNSHIELDED_SUBSCRIPTION = `subscription UnshieldedTransactions(
  $address: UnshieldedAddress!
  $transactionId: Int
) {
  unshieldedTransactions(address: $address, transactionId: $transactionId) {
    __typename
    ... on UnshieldedTransactionsProgress { highestTransactionId }
    ... on UnshieldedTransaction {
      transaction {
        type: __typename
        id
        ... on SystemTransaction { block { timestamp } }
        ... on RegularTransaction { transactionResult { status } }
      }
      createdUtxos { ...utxoFields }
      spentUtxos { ...utxoFields }
    }
  }
}

fragment utxoFields on UnshieldedUtxo {
  value
  owner
  tokenType
  intentHash
  outputIndex
  ctime
  registeredForDustGeneration
}`;

export interface StreamPlan {
  readonly document: string;
  readonly variables: Readonly<Record<string, unknown>>;
  readonly root: string;
}

export function planFor(
  stream: MidnightSyncStream,
  fromOffset: number,
  unshieldedAddress: string,
): StreamPlan {
  if (stream === "shielded") {
    return {
      document: SHIELDED_SUBSCRIPTION,
      variables: { id: fromOffset },
      root: "zswapLedgerEvents",
    };
  }
  if (stream === "dust") {
    return {
      document: DUST_SUBSCRIPTION,
      variables: { id: fromOffset },
      root: "dustLedgerEvents",
    };
  }
  return {
    document: UNSHIELDED_SUBSCRIPTION,
    variables: { address: unshieldedAddress, transactionId: fromOffset },
    root: "unshieldedTransactions",
  };
}

function record(value: unknown): Readonly<Record<string, unknown>> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new MidnightRuntimeError("INVALID_ARGUMENT");
  }
  return value as Readonly<Record<string, unknown>>;
}

function requireNumber(value: unknown): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value)) {
    throw new MidnightRuntimeError("INVALID_ARGUMENT");
  }
  return value;
}

function requireString(value: unknown): string {
  if (typeof value !== "string") {
    throw new MidnightRuntimeError("INVALID_ARGUMENT");
  }
  return value;
}

function decodeHex(value: string): Uint8Array {
  const hex = value.startsWith("0x") ? value.slice(2) : value;
  if (
    hex.length === 0 ||
    hex.length % 2 !== 0 ||
    !/^[0-9a-fA-F]+$/u.test(hex)
  ) {
    throw new MidnightRuntimeError("INVALID_ARGUMENT");
  }
  const bytes = new Uint8Array(hex.length / 2);
  for (let index = 0; index < bytes.length; index += 1) {
    bytes[index] = Number.parseInt(hex.slice(index * 2, index * 2 + 2), 16);
  }
  return bytes;
}

export interface DecodedEvent {
  readonly id: number;
  readonly payload: Uint8Array;
}

/** Ledger streams: hex-decode `raw` into the bytes the runtime replays. */
function decodeLedgerEvent(node: unknown): DecodedEvent {
  const event = record(node);
  return {
    id: requireNumber(event.id),
    payload: decodeHex(requireString(event.raw)),
  };
}

/** Projects a server UTXO down to exactly the seven fields WireUtxo accepts. */
function projectUtxo(node: unknown): Readonly<Record<string, unknown>> {
  const utxo = record(node);
  return {
    value: requireString(utxo.value),
    owner: requireString(utxo.owner),
    tokenType: requireString(utxo.tokenType),
    intentHash: requireString(utxo.intentHash),
    outputIndex: requireNumber(utxo.outputIndex),
    ctime: utxo.ctime === null ? null : requireNumber(utxo.ctime),
    registeredForDustGeneration: utxo.registeredForDustGeneration === true,
  };
}

/** Projects a transaction down to WireTransaction's four accepted fields. */
function projectTransaction(node: unknown): Readonly<Record<string, unknown>> {
  const transaction = record(node);
  const projected: Record<string, unknown> = {
    id: requireNumber(transaction.id),
    type: requireString(transaction.type),
  };
  if (transaction.block !== undefined && transaction.block !== null) {
    projected.block = {
      timestamp: requireNumber(record(transaction.block).timestamp),
    };
  }
  const result = transaction.transactionResult;
  if (result !== undefined && result !== null) {
    projected.transactionResult = {
      status: requireString(record(result).status),
    };
  }
  return projected;
}

const encoder = new TextEncoder();

/**
 * Unshielded stream: JSON tagged by `type`. Returns the id used for offsets —
 * a transaction's own id, or the progress marker's highest id.
 */
function decodeUnshieldedEvent(node: unknown): DecodedEvent {
  const event = record(node);
  const typename = requireString(event.__typename);
  if (typename === "UnshieldedTransactionsProgress") {
    const highest = requireNumber(event.highestTransactionId);
    return {
      id: highest,
      payload: encoder.encode(
        JSON.stringify({
          type: "UnshieldedTransactionsProgress",
          highestTransactionId: highest,
        }),
      ),
    };
  }
  if (typename !== "UnshieldedTransaction") {
    throw new MidnightRuntimeError("INVALID_ARGUMENT");
  }
  const transaction = projectTransaction(event.transaction);
  const created = Array.isArray(event.createdUtxos) ? event.createdUtxos : [];
  const spent = Array.isArray(event.spentUtxos) ? event.spentUtxos : [];
  return {
    id: requireNumber(transaction.id),
    payload: encoder.encode(
      JSON.stringify({
        type: "UnshieldedTransaction",
        transaction,
        createdUtxos: created.map(projectUtxo),
        spentUtxos: spent.map(projectUtxo),
      }),
    ),
  };
}

export function decodeEvent(
  stream: MidnightSyncStream,
  node: unknown,
): DecodedEvent {
  return stream === "unshielded"
    ? decodeUnshieldedEvent(node)
    : decodeLedgerEvent(node);
}
