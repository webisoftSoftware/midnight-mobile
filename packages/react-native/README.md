# Midnight Mobile

`@1am/midnight-mobile` is a community-maintained wallet-core runtime for Expo 55
and React Native 0.83 development builds. The package publishes compiled
ECMAScript modules, TypeScript declarations, Expo autolinking metadata, and the
reviewed native bridge sources, and prebuilt Apple and Android wallet runtime
libraries.

The package embeds a dynamic XCFramework for iOS device and simulator builds and
`.so` libraries for Android `arm64-v8a` and `x86_64`. Consumers do not run Cargo
or download binaries during installation. The Expo example validates both
platform bundles and runs the complete wallet lifecycle against its injected
mock runtime.

This alpha is experimental, unofficial, and not production-ready. The current
candidate has not been published. Final copyright, attribution, licensing,
notice, and distribution terms remain an M6 destination-repository gate; do not
distribute this private candidate as a release.

## Install in an Expo development build

The native module cannot run in Expo Go. After the reviewed package is
published, install its exact version and regenerate the native projects:

```sh
npm install --save-exact @1am/midnight-mobile@0.1.0-alpha.1
npx expo prebuild --clean
npx expo run:ios
npx expo run:android
```

Before publication, use only a reviewed local npm tarball in a controlled
evaluation. See the repository [quick start](../../docs/QUICK_START.md) for the
clean-project procedure and [compatibility matrix](../../docs/COMPATIBILITY.md)
for the exact tested toolchain. Expo SDK 55, Expo Modules Core 55, React 19.2,
React Native 0.83, iOS 15.1+, and Android API 24+ are the supported alpha line.

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

## Experimental mobile local prover

The separate `@1am/midnight-mobile/local-prover` entrypoint configures
application-supplied proof parameters and circuit files, executes the exact
Ledger 8.1.0 binary `/check` and `/prove` operations locally, and can route only
those two effects through the standard transport. Android maps uncompressed APK
assets or absolute sandbox files read-only; iOS maps bundle resources or
absolute application-sandbox files read-only. Both validate declared sizes and
SHA-256 hashes. The SDK does not bundle or download proof artifacts.

This entrypoint has no cancellation contract and does not handle
`proveAndBalance`, `balance`, or local fee balancing. Those effects remain
remote. Applications must close the prover after outstanding operations settle.
See the platform guides for [Android](../../docs/LOCAL_PROVER_ANDROID.md) and
[iOS](../../docs/LOCAL_PROVER_IOS.md) artifact, memory, error, and lifecycle
details.

## Maintainer verification

`npm run check:package` builds the package twice, verifies byte-for-byte
identical output, inspects the real npm tarball allowlist, requires the exact M4
native payload paths, installs the tarball into an isolated offline consumer,
resolves its declarations against the exact root-locked peer versions, and
compiles the consumer against the packed declarations.

`npm run build:native` builds and inspects the pinned Apple and Android targets
twice. `npm run check:native` verifies the standalone archives and checksums,
assembles the npm tarball reproducibly, and performs clean release consumer
builds with Rust unavailable. The repository's `docs/NATIVE_DISTRIBUTION.md`
records the supported architectures, size budgets, and release artifact
locations.

Use `npm run generate:bindings` only after an intentional Rust ABI change. It
builds the locked runtime, generates Swift and Kotlin twice, verifies
byte-for-byte reproducibility and the exact eight-function ABI, and updates the
reviewed package sources. `npm run check:bindings` performs the same
verification without modifying files.

## Documentation

- [Quick start](../../docs/QUICK_START.md)
- [Architecture and resumable effects](../../docs/ARCHITECTURE.md)
- [Public API and errors](../../docs/API_REFERENCE.md)
- [Network and proof-server configuration](../../docs/NETWORK_CONFIGURATION.md)
- [Experimental Android local prover](../../docs/LOCAL_PROVER_ANDROID.md)
- [Experimental iOS local prover](../../docs/LOCAL_PROVER_IOS.md)
- [Checkpoint storage and encryption](../../docs/CHECKPOINTS.md)
- [Seed lifecycle threat model](../../docs/SEED_THREAT_MODEL.md)
- [Compatibility matrix](../../docs/COMPATIBILITY.md)
- [Binary provenance](../../docs/BINARY_PROVENANCE.md)
- [Alpha limitations, support, and upgrades](../../docs/ALPHA_SUPPORT.md)
- [Security policy](../../SECURITY.md)
- [Unofficial project disclaimer](../../docs/UNOFFICIAL_PROJECT_DISCLAIMER.md)
