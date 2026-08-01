# Public API and error reference

This document describes the supported TypeScript surface of
`@1am/midnight-mobile@0.1.0-alpha.1`. Import all values and types from the
package root:

```ts
import {
  MidnightRuntimeController,
  createMidnightRuntimeApi,
  createStandardMidnightTransport,
} from "@1am/midnight-mobile";
```

Generated Swift/Kotlin bindings and the direct UniFFI ABI are internal package
implementation details and have no alpha compatibility guarantee.

## Encoding conventions

- Byte fields named `bytes`, payload arrays, and secret inputs are `Uint8Array`.
- Fields named `Base64` are canonical padded base64 strings.
- Hashes and keys named `Hex` are lowercase hexadecimal.
- Represent token amounts and other potentially large unsigned integers as
  unsigned decimal strings to avoid JavaScript precision loss. SDK outputs and
  inputs explicitly described as canonical use `"0"` or a nonzero digit followed
  by digits.
- IDs, generations, offsets, limits, and percentages are non-negative safe
  JavaScript integers unless a narrower constraint is documented.
- Caller-provided objects are treated as input only. Do not mutate them while an
  asynchronous call is pending.

## Primary construction APIs

### `createMidnightRuntimeApi`

```ts
function createMidnightRuntimeApi(
  loader?: NativeRuntimeModuleLoader,
): MidnightRuntimeApi;
```

Creates the typed wrapper around the Expo native module. Omit `loader` in an
application. The optional loader exists for deterministic testing and custom
module resolution.

The wrapper validates handles, exact 32-byte secret lengths, sync ranges, native
JSON, and every native result. It clones secret inputs before crossing the
bridge and wipes those copies after `openWalletSession` settles. Missing or
incomplete native modules fail with `UNAVAILABLE`.

### `MidnightRuntimeController`

```ts
new MidnightRuntimeController({
  api,
  transport,
  checkpointStore?,
  logger?,
});
```

The recommended imperative API. It combines `MidnightRuntimeApi`,
`MidnightStandardTransport`, optional checkpoint persistence, and lifecycle
bookkeeping.

| Member                                            | Contract                                                                                                  |
| ------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| `status`                                          | Current `MidnightRuntimeStatus`.                                                                          |
| `subscribe(listener)`                             | Calls `listener` on later status changes; returns an unsubscribe function.                                |
| `openWalletSession(config, secrets, checkpoint?)` | Loads a stored checkpoint when the third argument is omitted, or uses the explicit checkpoint/null value. |
| `applySyncBatch(session, batch)`                  | Applies one contiguous batch; persists a non-duplicate result.                                            |
| `getWalletSnapshot(session)`                      | Returns the current typed snapshot.                                                                       |
| `runCommand(session, command, options?)`          | Drives the complete bounded effect loop and returns the command-specific result.                          |
| `pause()`                                         | Aborts transport work, cancels tracked operations, and persists every session.                            |
| `refresh()`                                       | Re-reads snapshots and recomputes controller status.                                                      |
| `closeWalletSession(session)`                     | Cancels work, persists, closes native state, and invalidates the handle.                                  |
| `dispose()`                                       | Closes every controller-owned session.                                                                    |

`MidnightRuntimeControllerOptions` is the constructor object shown above.
`MidnightRunCommandOptions<K>` contains an optional `AbortSignal` and an
optional synchronous or asynchronous `onStep(step)` callback.

The transport awaits `onStep`. If the callback rejects, the active native
operation is cancelled and the callback error propagates. Keep progress handling
bounded, non-sensitive, and safe to fail.

### `createStandardMidnightTransport`

```ts
function createStandardMidnightTransport(
  config: MidnightTransportConfiguration,
): MidnightStandardTransport;
```

Creates the direct indexer/proof/node transport. Configuration and its network
outcomes are defined in [network configuration](./NETWORK_CONFIGURATION.md).

`MidnightStandardTransport` exposes:

- `runCommand(api, session, command, options?)`, which drives the resumable
  command to a typed result; and
- `openSyncSocket(onPayload, signal?)`, which opens the configured indexer
  WebSocket and emits decoded bytes.

### React provider

```tsx
<MidnightRuntimeProvider
  api={api}
  transport={transport}
  checkpointStore={checkpointStore}
  logger={logger}
  onError={reportLifecycleError}
>
  <Application />
</MidnightRuntimeProvider>
```

`MidnightRuntimeProviderProps` extends `MidnightRuntimeControllerOptions` with
optional `children` and `onError`. The provider pauses on inactive/background
app states, refreshes on activation, and disposes on unmount.

`useMidnightRuntime()` returns `MidnightRuntimeContextValue`:

```ts
{
  controller: MidnightRuntimeController;
  status: MidnightRuntimeStatus;
  error: MidnightRuntimeError | null;
}
```

Calling the hook outside the provider throws a normal JavaScript `Error`.
`error` and `onError` report provider lifecycle failures; command and session
callers must still handle their returned promise rejections directly.

## Runtime session API

`MidnightRuntimeApi` is the low-level typed native interface:

```ts
interface MidnightRuntimeApi {
  openWalletSession(
    config,
    secrets,
    checkpoint?,
  ): Promise<MidnightSessionHandle>;
  applySyncBatch(session, batch): Promise<MidnightApplySyncResult>;
  getWalletSnapshot(session): Promise<MidnightWalletSnapshot>;
  exportWalletCheckpoint(session): Promise<MidnightCheckpoint>;
  beginCommand(session, command): Promise<MidnightOperationStep>;
  resumeOperation(operation, networkResult): Promise<MidnightOperationStep>;
  cancelOperation(operation): Promise<void>;
  closeWalletSession(session): Promise<void>;
}
```

Prefer the controller unless implementing a custom effect executor. A custom
executor must preserve effect IDs, cancellation, generation fencing, submission
ambiguity, and the one-operation-per-session rule described in
[architecture](./ARCHITECTURE.md).

### Session configuration and secrets

`MidnightNetworkId` is `preview`, `preprod`, or `mainnet`. `MidnightSyncStream`
is `shielded`, `unshielded`, or `dust`.

```ts
interface MidnightWalletSessionConfig {
  readonly networkId: "preview" | "preprod" | "mainnet";
  readonly walletFingerprint: string;
  readonly unshieldedAddress: string;
}

interface MidnightWalletSessionSecrets {
  readonly nightExternalKey: Uint8Array;
  readonly zswapSeed: Uint8Array;
  readonly dustSeed: Uint8Array;
}
```

`walletFingerprint` and `unshieldedAddress` must be nonblank and at most 256
bytes as represented by the native JSON string. Each secret must contain exactly
32 bytes. The runtime does not verify that the supplied unshielded address
corresponds to `nightExternalKey`; the adopter's audited key-management
component must keep them consistent. The fingerprint is a stable, non-secret
application identifier and must not contain seed material. The caller owns and
must wipe the original secret arrays.

`MidnightSessionHandle` and `MidnightOperationHandle<K>` contain numeric `id`
and `generation`; operation handles also contain `commandKind`. They are opaque,
process-local capabilities. Do not persist, synthesize, compare by ID alone, or
reuse them after close/cancel.

The native runtime permits two open sessions and one active operation per
session.

### Synchronization

```ts
type MidnightSyncInputStream =
  "shielded" | "unshielded" | "dust" | "shielded-v2" | "dust-v2";

interface MidnightSyncBatch {
  readonly stream: MidnightSyncInputStream;
  readonly fromOffset: number;
  readonly toOffset: number;
  readonly payloads: readonly Uint8Array[];
}

interface MidnightApplySyncResult {
  readonly duplicate: boolean;
  readonly snapshot: MidnightWalletSnapshot;
}
```

`fromOffset` must match the current `MidnightStreamOffset.nextOffset`.
`toOffset` must not be smaller. Exact retained replays return `duplicate: true`;
gaps or conflicting overlap return `SYNC_GAP`. Total batch payload is limited to
64 MiB by native validation.

Apply every data page with its non-empty payload array. When the selected
indexer confirms the stream is at its current tip, apply one terminal batch:
`payloads: []` and `fromOffset === toOffset === nextOffset`. The
TypeScript/native boundary translates that public convention into the internal
`<stream>-tip` marker. Internal marker names are not public stream values; never
cast or pass one from application code.

`shielded-v2` and `dust-v2` accept protocol-v2 response pages. They are input
discriminants only: offsets and caught-up status remain reported under the
canonical `shielded` and `dust` streams, and terminal empty batches use those
canonical names.

The snapshot becomes `ready` only after `shielded`, `unshielded`, and `dust`
have each received a terminal batch. Later nonterminal data clears that stream's
caught-up state.

### Checkpoint

```ts
interface MidnightCheckpoint {
  readonly version: 1;
  readonly bytes: Uint8Array;
}
```

The bytes are opaque and privacy-sensitive. See
[checkpoint storage](./CHECKPOINTS.md).

### Snapshot

`MidnightWalletSnapshot` contains:

| Field                            | Type and meaning                                              |
| -------------------------------- | ------------------------------------------------------------- |
| `networkId`                      | `preview`, `preprod`, or `mainnet`.                           |
| `walletFingerprint`              | The application identifier supplied at open.                  |
| `status`                         | `syncing` until all streams are caught up, otherwise `ready`. |
| `generation`                     | Current native generation fence.                              |
| `streamOffsets`                  | Sorted `MidnightStreamOffset[]` with stream and next offset.  |
| `unshieldedAddress`              | Address supplied at open.                                     |
| `shieldedCoinPublicKeyHex`       | 32-byte lowercase hex public key.                             |
| `shieldedEncryptionPublicKeyHex` | 32-byte lowercase hex public key.                             |
| `dustPublicKey`                  | Canonical decimal public key value.                           |
| `balances`                       | `MidnightWalletBalances`.                                     |
| `pendingSubmissions`             | Sorted `MidnightPendingSubmission[]`.                         |

`MidnightWalletBalances` contains token-to-decimal-string
`shieldedBalances`/`unshieldedBalances`, total shielded/unshielded values,
`dustBalance`, `availableUtxos`, `dustGeneratingNight`, and
`MidnightDustCoinSnapshot[]`.

Each dust coin has a decimal `nonce`, `generatedNow`, `maxCap`, nullable
`maxCapReachedAt`, and `maturing` flag.

Each pending submission has a lowercase 32-byte `transactionHash`, hex
`identifiers`, and status `awaitingResponse`, `accepted`, `rejected`, or
`statusUnknown`.

## Operation steps and network results

`MidnightOperationStep<K>` is a discriminated union:

- `progress`: operation handle, phase, and optional non-negative percent;
- `network`: operation handle, unique effect ID, effect, endpoint role, and
  opaque base64 body; or
- `complete`: optional final handle and `MidnightCommandResult<K>`.

`MidnightNetworkEffect` is `sync`, `check`, `prove`, `proveAndBalance`,
`balance`, `submit`, or `confirm`. `MidnightEndpointRole` is `indexer`, `proof`,
or `node`.

A custom executor resumes a network step with:

```ts
interface MidnightNetworkResult {
  readonly effectId: string;
  readonly outcome: "accepted" | "rejected" | "statusUnknown";
  readonly bodyBase64?: string;
}
```

The `effectId` must exactly match the pending effect. Only a submission may
legitimately use `statusUnknown` without being reduced to a proof or transport
failure.

## Commands and typed results

`MidnightCommandKind` is the union of exactly 22 command names.
`MIDNIGHT_COMMAND_KINDS` is the corresponding readonly runtime array.
`MidnightCommandMap` and `MidnightCommandResultMap` provide the exhaustive
input/result mapping. The generic aliases are:

```ts
type MidnightCommand<K> = MidnightCommandMap[K] & { readonly kind: K };
type MidnightCommandResult<K> = MidnightCommandResultMap[K];
type MidnightRuntimeCommand = MidnightCommandMap[MidnightCommandKind];
```

### Signing and codecs

| Kind                      | Input after `kind`                                                | Result                                                                        |
| ------------------------- | ----------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| `signData`                | `domain`, `dataBase64`                                            | `MidnightSignatureResult`: 64-byte `signatureHex`, 32-byte `verifyingKeyHex`. |
| `createCheckPayload`      | `preimageBase64`, optional `irBase64`                             | `MidnightPayloadResult`: `payloadBase64`.                                     |
| `parseCheckResult`        | `resultBase64`                                                    | `MidnightCheckResult`: decimal-string/null `values`.                          |
| `createProvingPayload`    | `preimageBase64`, optional `bindingInput`, optional `keyMaterial` | `MidnightPayloadResult`.                                                      |
| `canonicalizeTransaction` | signature/proof/binding markers and `rawBase64`                   | `MidnightCanonicalTransactionResult`: `transactionBase64`.                    |

`signData.domain` must be nonblank and at most 1,024 bytes; data is limited to 1
MiB. The signature covers a versioned, length-prefixed domain/data transcript,
not the raw data alone.

`MidnightProvingKeyMaterial` contains `proverKeyBase64`, `verifierKeyBase64`,
`irBase64`, and optional `compression: "gzip"`.

Canonicalization markers are:

- signature: `signature` or `signature-erased`;
- proof: `proof`, `pre-proof`, or `no-proof`; and
- binding: `binding`, `pre-binding`, or `no-binding`.

### Sync and protocol helpers

| Kind                          | Input after `kind`                                       | Result                                                                   |
| ----------------------------- | -------------------------------------------------------- | ------------------------------------------------------------------------ |
| `createSyncRequest`           | standard stream/offset/limit, or fast shielded/dust mode | `MidnightSyncRequestResult`: stream, offset, request bytes.              |
| `deriveShieldedMintContext`   | none                                                     | `MidnightShieldedMintContextResult`.                                     |
| `watchShieldedMint`           | `coinInfoBase64`, `expectedOutputIndex`                  | `MidnightWatchShieldedMintResult`.                                       |
| `createShieldedSpentRequest`  | none                                                     | `MidnightShieldedSpentRequestResult`: request bytes and nullifier count. |
| `applyShieldedSpentResponse`  | `resultBase64`                                           | `MidnightShieldedSpentResult`: removed count.                            |
| `setShieldedProtocolVersion`  | `protocolVersion`, `syncOffset`                          | `MidnightProtocolVersionResult`.                                         |
| `createDustSpendRequest`      | none                                                     | `MidnightDustSpendRequestResult`: request bytes and UTXO count.          |
| `createDustCommitmentRequest` | `rawBase64`, `syncOffset`                                | `MidnightDustCommitmentResult`.                                          |
| `applyDustSpendResolution`    | `rawBase64`, `resultBase64`, `syncOffset`                | `MidnightDustResolutionResult`: `status: "applied"`.                     |

Standard `createSyncRequest.limit` is from 1 through 4,096. Fast mode omits the
limit and accepts only `shielded` or `dust`. Offset-sensitive commands fail with
`SYNC_GAP` if the supplied offset differs from native state.

`MidnightDustCommitmentResult` is one of `status: "ahead"`,
`status: "unchanged"`, or `status: "rebuild"` with `requestBase64`.

### Transactions

| Kind                          | Input after `kind`                                               | Result                                |
| ----------------------------- | ---------------------------------------------------------------- | ------------------------------------- |
| `transfer`                    | recipient, decimal amount, token type, wallet type               | `MidnightFinalizedTransactionResult`. |
| `dappTransfer`                | output array                                                     | `MidnightFinalizedTransactionResult`. |
| `dappIntent`                  | input and output arrays                                          | `MidnightFinalizedTransactionResult`. |
| `generateDust`                | ledger parameters, fee margin, decimal overhead                  | `MidnightFinalizedTransactionResult`. |
| `balanceUnsealed`             | raw transaction, ledger parameters, fee margin, decimal overhead | `MidnightFinalizedTransactionResult`. |
| `balanceSealed`               | raw transaction, ledger parameters, fee margin, decimal overhead | `MidnightFinalizedTransactionResult`. |
| `finalizeUnprovenTransaction` | `rawBase64`, optional key-location-to-material record            | `MidnightFinalizedTransactionResult`. |
| `submitFinalized`             | `rawBase64`                                                      | `MidnightSubmissionResult`.           |

`MidnightWalletType` is `shielded` or `unshielded`. `MidnightDappInput` contains
wallet type, token type, and decimal amount. `MidnightDappOutput` adds
`receiverAddress`.

Fee-block margins may not exceed 64. `additionalFeeOverhead` must be a canonical
unsigned decimal string.

`finalizeUnprovenTransaction.keyMaterial` maps exact Ledger proof key locations
to `MidnightProvingKeyMaterial`. If supplied, the record must be non-empty; its
locations and artifact fields must be non-empty, and every location must occur
in the unproven transaction. Matching IR is attached only to its `check`
request, while matching full key material is attached only to its `prove`
request. Decoded private artifact buffers are zeroized when the operation ends.

`MidnightFinalizedTransactionResult` contains:

- `transactionBase64`;
- 32-byte `transactionHash`;
- 32-byte `ledgerTransactionHash`;
- hex `identifiers`; and
- optional numeric `expiresAt`.

`MidnightSubmissionResult` contains transaction hash, identifiers, an `accepted`
or `rejected` status, and optional response `bodyBase64`. An ambiguous result
does not return this type; it throws `SUBMISSION_STATUS_UNKNOWN`.

Transaction-building commands require all sync streams to be caught up. Their
proof/balance effects may require multiple network round trips. `transfer`,
`dappTransfer`, and `finalizeUnprovenTransaction` emit individual `check` and
`prove` effects followed by a remote `balance` effect containing the locally
proved and sealed transaction. Proposed wallet state is committed only after the
accepted balance response passes transaction, identifier, and hash checks.

## Host interfaces

### `MidnightCheckpointStore`

```ts
interface MidnightCheckpointStore {
  load(key: string): Promise<MidnightCheckpoint | null>;
  save(key: string, checkpoint: MidnightCheckpoint): Promise<void>;
  remove(key: string): Promise<void>;
}
```

The controller key is `<networkId>:<walletFingerprint>`.
`InMemoryMidnightCheckpointStore` clones bytes and is only for tests/examples.
It is not encrypted or persistent.

### `MidnightLogger`

`MidnightLogger.write(event)` receives `MidnightLogEvent`. Levels are `debug`,
`info`, `warn`, or `error`. Event names are:

- `session.open`, `session.close`, `session.pause`;
- `sync.apply`, `operation.step`; and
- `transport.request`, `transport.response`, `transport.failure`.

Optional metadata is limited to endpoint role, effect, outcome, duration, byte
length, and error code. `write` must return promptly and must not throw.
`silentMidnightLogger` discards all events.

## Transport interfaces

`MidnightTransportConfiguration` contains:

- required `network: MidnightNetworkConfiguration`;
- required `fetch: MidnightFetch`;
- required `createWebSocket: MidnightWebSocketFactory`;
- optional async per-role `headers`;
- optional `timeoutMs`;
- optional `maximumEffectSteps`; and
- optional `logger`.

`MidnightFetchRequest` is a `POST` with headers, `Uint8Array` body, and
`AbortSignal`. `MidnightFetchResponse` exposes numeric `status` and async
`arrayBuffer()`.

`MidnightWebSocket` is the minimal interface required by the SDK: `readyState`,
`send`, `close`, and `addEventListener` for open/error/close/message.
`MidnightWebSocketFactory` receives the configured URL.

## Status model

`MidnightRuntimeStatus` is:

| Status    | Meaning                                                   |
| --------- | --------------------------------------------------------- |
| `idle`    | No controller-owned open session.                         |
| `opening` | A session open is pending.                                |
| `syncing` | At least one session is not fully caught up.              |
| `ready`   | The observed session or all refreshed sessions are ready. |
| `closing` | A session close is pending.                               |
| `error`   | Session open failed.                                      |

Status is controller-wide and informational. Use snapshots for wallet-specific
state and handle asynchronous errors from each call.

## Error model

All recognized SDK/native failures are `MidnightRuntimeError` with a stable
`code: MidnightRuntimeErrorCode`. Messages contain the same code and no
sensitive native detail. Unknown native failures normalize to `NATIVE_INTERNAL`.

| Code                        | Meaning and caller response                                                                                                                                                                             |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `INVALID_ARGUMENT`          | Malformed value, unsupported command/marker, invalid URL/configuration, noncanonical encoding, or effect mismatch. Correct the request; do not retry unchanged.                                         |
| `STALE_SESSION`             | Closed, unknown, mismatched-generation, or obsolete session/operation handle. Discard it and re-open if appropriate.                                                                                    |
| `CANCELLED`                 | Caller/lifecycle cancellation or an already-cancelled operation. Treat as terminal for that operation.                                                                                                  |
| `STATE_INCOMPATIBLE`        | Checkpoint magic, version, Ledger revision, checksum, wallet, network, or structure is incompatible. Quarantine the bytes; do not silently reset production state.                                      |
| `SYNC_GAP`                  | Batch or offset-sensitive command does not match current contiguous state. Re-read the snapshot and repair indexer sequencing.                                                                          |
| `PROOF_FAILED`              | Proof/balance service rejected, returned ambiguous/malformed/inconsistent data, or failed runtime verification. Do not commit or submit the proposed transaction.                                       |
| `INSUFFICIENT_DUST`         | Available DUST cannot satisfy the requested fee/balance operation. Refresh state and change the operation or funding.                                                                                   |
| `SUBMISSION_STATUS_UNKNOWN` | Submission may have reached the node but no definitive response was obtained. Reconcile the recorded transaction hash; never treat it as rejection.                                                     |
| `TRANSPORT_ERROR`           | A non-submission network effect failed or was ambiguous. Retry only under an adopter-defined safe policy.                                                                                               |
| `NATIVE_INTERNAL`           | Native response decoding, effect bound, persistence aggregate, lock, serialization, or unexpected internal failure. Preserve redacted diagnostics and report a bug.                                     |
| `UNAVAILABLE`               | Native module absent/incomplete, two-session cap reached, session already has active work, wallet not caught up, or another temporary runtime precondition is unmet. Correct the state before retrying. |

Storage adapters may throw arbitrary errors. Open and close normalize storage
failures, but errors during sync, command-step persistence, or pause can
propagate unchanged. Injected `fetch`, WebSocket, logger, header resolver, and
callbacks also remain application code and can throw their own error types.
Implement these boundaries defensively, handle unknown failures, and never put
sensitive text in an exception.

## Native module exports

`EXPO_MIDNIGHT_NATIVE_MODULE_NAME` is `"MidnightMobileRuntime"`.
`NativeRuntimeModule` and `NativeRuntimeModuleLoader` describe the eight-method
bridge consumed by `createMidnightRuntimeApi`. They are exported to support
tests and module loading, not as stable direct-native APIs. Applications should
not call them or generated bindings directly.
