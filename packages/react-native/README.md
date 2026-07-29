# Midnight Mobile

`@1am/midnight-mobile` is a community-maintained wallet-core runtime for Expo 55
and React Native 0.83 development builds. The package publishes compiled
ECMAScript modules, TypeScript declarations, Expo autolinking metadata, and the
reviewed native bridge sources.

The repository's M3 package intentionally has no prebuilt native runtime.
Platform binaries and clean native release builds are M4 deliverables. The M3
Expo example therefore validates both platform bundles and runs the complete
wallet lifecycle against its injected mock runtime.

## Host configuration

Applications create a `MidnightRuntimeController` from an API, transport, and
optional host adapters. The standard transport has no endpoint or credential
defaults:

```ts
import {
  InMemoryMidnightCheckpointStore,
  MidnightRuntimeController,
  createMidnightRuntimeApi,
  createStandardMidnightTransport,
} from "@1am/midnight-mobile";

const transport = createStandardMidnightTransport({
  network: {
    indexerHttpUrl: adopterConfiguration.indexerHttpUrl,
    indexerWebSocketUrl: adopterConfiguration.indexerWebSocketUrl,
    proofServerUrl: adopterConfiguration.proofServerUrl,
    nodeUrl: adopterConfiguration.nodeUrl,
  },
  fetch: adopterFetch,
  createWebSocket: adopterWebSocketFactory,
  headers: async (role) => adopterHeadersFor(role),
});

const controller = new MidnightRuntimeController({
  api: createMidnightRuntimeApi(),
  transport,
  checkpointStore: new InMemoryMidnightCheckpointStore(),
});
```

`MidnightRuntimeProvider` and `useMidnightRuntime` expose the controller through
React context. The provider pauses active work while the app is inactive,
refreshes it on activation, and disposes it when unmounted.

HTTP headers are resolved for each `indexer`, `proof`, or `node` request.
WebSocket credentials remain under adopter control by closing over them in
`createWebSocket`. Both injected implementations must honor the supplied abort
signal or socket close request. Progress is reported through `onStep`; caller
cancellation is reported as `CANCELLED`.

## Network outcomes

The runtime effect chooses the endpoint role. The transport never silently
switches endpoints or retries an operation.

| Condition                                     | Submission effect | Other network effect |
| --------------------------------------------- | ----------------- | -------------------- |
| HTTP 2xx                                      | `accepted`        | `accepted`           |
| HTTP 4xx                                      | `rejected`        | `rejected`           |
| HTTP 3xx or 5xx                               | `statusUnknown`   | `TRANSPORT_ERROR`    |
| Connection, response-body, or timeout failure | `statusUnknown`   | `TRANSPORT_ERROR`    |
| Caller abort                                  | `CANCELLED`       | `CANCELLED`          |

An ambiguous submission is passed back to the native operation as
`statusUnknown`; it must not be reported as accepted or retried as though it
were rejected. A malformed endpoint, non-positive timeout, or non-positive
effect limit fails synchronously with `INVALID_ARGUMENT`. Exceeding the bounded
effect loop cancels the native operation and reports `NATIVE_INTERNAL`.

Logger events contain only the endpoint role, effect, outcome, duration, byte
length, and stable error code. They never contain URLs, headers, credentials,
wallet material, checkpoints, or request and response bodies.

## Checkpoints and secrets

Checkpoints are opaque privacy-sensitive bytes.
`InMemoryMidnightCheckpointStore` clones bytes and is intended only for examples
and tests. Production applications must inject encrypted, authenticated,
device-protected storage and define their own retention policy. The package does
not bundle persistent storage.

Session seeds cross JavaScript and the native bridge. The runtime and bridge
best-effort wipe their temporary copies, but the caller owns the original
buffers and must clear them after `openWalletSession` settles. Never place
seeds, checkpoints, credentials, signatures, or transaction payloads in logs.

## Maintainer verification

`npm run check:package` builds the package twice, verifies byte-for-byte
identical output, inspects the real npm tarball allowlist, rejects premature M4
binaries, installs the tarball into an isolated offline consumer, resolves its
declarations against the exact root-locked peer versions, and compiles the
consumer against the packed declarations.

Use `npm run generate:bindings` only after an intentional Rust ABI change. It
builds the locked runtime, generates Swift and Kotlin twice, verifies
byte-for-byte reproducibility and the exact eight-function ABI, and updates the
reviewed package sources. `npm run check:bindings` performs the same
verification without modifying files.
