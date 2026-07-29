# Architecture and resumable effects

Midnight Mobile is a React Native-first wallet runtime. Its supported public
surface is TypeScript. Generated Swift and Kotlin bindings and the
eight-function UniFFI ABI are package internals.

## Trust and responsibility boundaries

```text
Application UI and lifecycle
        |
        v
MidnightRuntimeProvider / MidnightRuntimeController
        |                    |
        |                    +--> adopter CheckpointStore and Logger
        v
createStandardMidnightTransport
        |
        +--> adopter fetch and WebSocket implementations
        |       |
        |       +--> adopter-selected indexer, proof server, and node
        v
createMidnightRuntimeApi
        |
        v
Expo Swift/Kotlin bridge --> generated UniFFI binding --> Rust wallet runtime
```

The boundaries are intentionally asymmetric:

- the application owns UI, wallet-secret acquisition, endpoint selection,
  credentials, background policy, storage, and logs;
- the controller owns JavaScript session bookkeeping, lifecycle cancellation,
  checkpoint timing, and status notifications;
- the standard transport owns bounded effect execution, timeout and abort
  propagation, endpoint-role routing, and conservative network outcomes;
- the Expo bridge converts native values and wipes temporary native buffers;
- Rust owns wallet state, validation, transaction construction, checkpoint
  encoding, generation fencing, command state machines, and session secrets.

Rust performs no network, persistent-storage, UI, or platform lifecycle I/O.

## Session model

`openWalletSession` validates the network and identifiers, derives wallet
address material, optionally restores a checkpoint, and returns an opaque
`{ id, generation }` handle.

The runtime supports at most two open sessions and at most one active command
operation per session. A session stores:

- its monotonically assigned generation;
- wallet-core state and three seed/key inputs;
- the next offset and caught-up status for each sync stream;
- recent batch receipts used for duplicate detection;
- pending submission records; and
- the currently active resumable operation, if any.

The generation is a fence, not an application sequence number. Every session and
operation call supplies both the native ID and its generation. Closed,
cancelled, mismatched, or otherwise obsolete handles fail instead of reaching
new state that happens to reuse an ID.

## Synchronization model

Synchronization uses three canonical streams: `shielded`, `unshielded`, and
`dust`. The host obtains data from its selected indexer and applies it with
`applySyncBatch`.

For each stream, the runtime enforces:

1. `fromOffset` equals the recorded next offset;
2. `toOffset` is not before `fromOffset`;
3. the stream-specific wire payload offsets are valid;
4. the entire batch is decoded and applied to cloned state;
5. wallet state, next offset, and the receipt commit together only after the
   whole data batch succeeds; and
6. a public terminal batch—empty `payloads` with equal offsets—is translated by
   the TypeScript/native wrapper into the internal `<stream>-tip` marker.

An exact replay of a retained receipt returns `duplicate: true` without
reapplying data. An overlapping batch with different bytes, a missing range, or
an invalid payload returns `SYNC_GAP` and commits nothing. Receipt history is
bounded, so applications must treat the current stream offsets—not indefinite
replay receipts—as authoritative.

The indexer integration applies every nonterminal page contiguously. Once the
indexer confirms a stream is at its current tip, it applies one empty terminal
batch with `fromOffset === toOffset === nextOffset`. Internal tip names are not
part of `MidnightSyncStream`, and applications never construct them. The native
status becomes `ready` only after all three streams receive a terminal batch.
Applying later nonterminal data clears that stream's caught-up state.

## Resumable effect protocol

Some commands complete synchronously in Rust. Commands requiring proof,
balancing, or submission return a serializable step instead:

```ts
type Step =
  | { kind: "progress"; operation: Handle; phase: string; percent?: number }
  | {
      kind: "network";
      operation: Handle;
      effectId: string;
      effect: Effect;
      endpointRole: "indexer" | "proof" | "node";
      bodyBase64: string;
    }
  | { kind: "complete"; operation: Handle | null; result: TypedResult };
```

The standard transport executes the protocol as follows:

1. Call `beginCommand(session, command)`.
2. Report the step through `onStep`.
3. For a network step, route the opaque body by `endpointRole`.
4. Convert the response into `accepted`, `rejected`, or `statusUnknown`.
5. Call `resumeOperation(operation, result)` with the exact `effectId`.
6. Repeat until a typed `complete` result is returned.

The default loop is bounded to 64 steps and each request to 30 seconds.
Applications may choose lower positive integer limits. Exceeding the effect
limit cancels the native operation and fails with `NATIVE_INTERNAL`.

`effectId` binds one response to one pending effect. A mismatched ID is rejected
and cannot advance the operation. Proof responses are validated against the
expected request and transaction identifiers before proposed wallet state is
committed.

## Effect routing

| Effect            | Role    | Meaning                                |
| ----------------- | ------- | -------------------------------------- |
| `sync`            | indexer | Standard synchronization request       |
| `check`           | proof   | Check request in a proof sequence      |
| `prove`           | proof   | Proving request in a proof sequence    |
| `proveAndBalance` | proof   | Wallet transfer proof/balance request  |
| `balance`         | proof   | Balance-service request                |
| `submit`          | node    | Finalized transaction submission       |
| `confirm`         | node    | Submission confirmation when requested |

The runtime chooses the role. The transport does not fail over to a different
role or endpoint.

## Cancellation and lifecycle

An `AbortSignal` cancels JavaScript network work and causes the controller to
cancel the tracked native operation. `cancelOperation` removes native pending
state. Cancellation is terminal for that operation handle.

`MidnightRuntimeProvider` observes React Native `AppState`:

- leaving `active` calls `pause`, which aborts transport work, cancels tracked
  operations, and exports checkpoints;
- returning to `active` calls `refresh`, which reads current native snapshots;
- unmounting disposes all tracked sessions.

The provider reports lifecycle errors but does not invent retry policy. A host
that does not use React can call the same controller methods directly.

## Submission ambiguity

Submission is not safely retryable merely because the HTTP response was lost.
The transport maps:

- HTTP 2xx to `accepted`;
- HTTP 4xx to `rejected`; and
- redirects, 5xx responses, timeouts, connection failures, and unreadable bodies
  to `statusUnknown`.

For `submit`, `statusUnknown` is resumed into Rust. Rust records the pending
submission as `statusUnknown` and throws `SUBMISSION_STATUS_UNKNOWN`. That
record is included in snapshots and checkpoints. The application must reconcile
the transaction hash before deciding whether to resubmit or construct a
replacement. Other effects map ambiguous transport outcomes to
`TRANSPORT_ERROR`.

## Checkpoint timing

When a `CheckpointStore` is injected, the controller exports a checkpoint:

- after a non-duplicate sync batch;
- at every reported command step;
- while pausing; and
- before closing.

This captures stream offsets, effect-adjacent state, and pending submissions. It
does not turn the operation handle itself into a stable cross-process resume
token. After restart, open from the checkpoint, inspect the snapshot, reconcile
pending submissions, and start a new supported command.

The storage and seed security contracts are documented separately in
[checkpoints](./CHECKPOINTS.md) and the
[seed threat model](./SEED_THREAT_MODEL.md).

## Design constraints

- Only the 19 public wallet-core commands are accepted.
- Service configuration and credentials never enter Rust configuration.
- Native responses are decoded into concrete TypeScript types before returning.
- Generated native bindings may change in any alpha without compatibility
  guarantees.
- No social, FT/NFT, gateway-authentication, private-fast-sync, verifier-asset,
  IPFS, or operated-service capability is present.
