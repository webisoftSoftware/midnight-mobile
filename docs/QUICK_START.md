# Quick start

This guide installs `@1am/midnight-mobile` in a clean Expo development build.
The package contains native Apple and Android libraries, so it cannot run in
Expo Go. The current alpha is not published; until the M6 release, use a
reviewed npm tarball produced from this repository.

Read the [unofficial project disclaimer](./UNOFFICIAL_PROJECT_DISCLAIMER.md)
before evaluating the SDK. The alpha is experimental wallet software, has no
operated services, and is not production-ready.

## Supported setup

Use all of the following:

- Node.js 22 and npm 11;
- Expo SDK 55, Expo Modules Core 55, React 19.2, and React Native 0.83;
- iOS 15.1 or later on an arm64 device, or an arm64/x86_64 simulator;
- Android API 24 or later on `arm64-v8a` or `x86_64`; and
- an Expo development build generated with `npx expo prebuild`.

The complete tested versions are in the
[compatibility matrix](./COMPATIBILITY.md).

## Create a clean application

Create the application outside this repository:

```sh
npx create-expo-app@latest midnight-evaluation \
  --template default@sdk-55
cd midnight-evaluation
```

Install the SDK from one of these sources:

```sh
# After the M6 npm release:
npm install --save-exact @1am/midnight-mobile@0.1.0-alpha.1

# Before publication, use the reviewed tarball:
npm install /absolute/path/to/1am-midnight-mobile-0.1.0-alpha.1.tgz
```

Do not install generated bindings, a CocoaPod, a Maven artifact, or a Rust crate
separately. Those are not supported consumer surfaces.

Generate clean native projects after installing the package:

```sh
npx expo prebuild --clean
npx expo run:ios
npx expo run:android
```

Re-run `npx expo prebuild --clean` after changing the SDK version or native app
configuration. A JavaScript-only `npx expo start` session is useful only after
the development build containing the module is installed.

## Supply the host boundary

The SDK does not choose an indexer, proof server, node, credential, persistent
store, logger, `fetch`, or WebSocket implementation. Provide each dependency
explicitly:

```ts
import {
  MidnightRuntimeController,
  createMidnightRuntimeApi,
  createStandardMidnightTransport,
  type MidnightCheckpointStore,
  type MidnightFetch,
  type MidnightWebSocketFactory,
} from "@1am/midnight-mobile";

declare const checkpointStore: MidnightCheckpointStore;
declare const midnightFetch: MidnightFetch;
declare const createWebSocket: MidnightWebSocketFactory;
declare const serviceConfiguration: {
  indexerHttpUrl: string;
  indexerWebSocketUrl: string;
  proofServerUrl: string;
  nodeUrl: string;
  headersFor(
    role: "indexer" | "proof" | "node",
  ): Promise<Readonly<Record<string, string>>>;
};

const transport = createStandardMidnightTransport({
  network: {
    indexerHttpUrl: serviceConfiguration.indexerHttpUrl,
    indexerWebSocketUrl: serviceConfiguration.indexerWebSocketUrl,
    proofServerUrl: serviceConfiguration.proofServerUrl,
    nodeUrl: serviceConfiguration.nodeUrl,
  },
  fetch: midnightFetch,
  createWebSocket,
  headers: serviceConfiguration.headersFor,
});

export const runtime = new MidnightRuntimeController({
  api: createMidnightRuntimeApi(),
  transport,
  checkpointStore,
});
```

Only `http:`/`https:` service URLs and `ws:`/`wss:` indexer socket URLs are
accepted. Production applications should require TLS and independently validate
every service operator. See [network configuration](./NETWORK_CONFIGURATION.md).

## Open and synchronize a wallet

All three secret inputs are exactly 32 bytes. They cross JavaScript and the
native bridge. The SDK wipes only the copies it owns; the caller must clear its
buffers after the open attempt settles.

The package does not accept a mnemonic or derive the application wallet
configuration. An audited adopter-owned key component must produce the three
required values and the network-valid unshielded address. The runtime validates
the address's shape but does not prove it corresponds to `nightExternalKey`. Use
a stable, non-secret wallet fingerprint; never use raw seed material as an
identifier.

```ts
const secrets = {
  nightExternalKey: obtainNightExternalKey(),
  zswapSeed: obtainZswapSeed(),
  dustSeed: obtainDustSeed(),
};

let session;
try {
  session = await runtime.openWalletSession(
    {
      networkId: "preview",
      walletFingerprint: "app-owned-stable-wallet-id",
      unshieldedAddress: "app-derived-network-address",
    },
    secrets,
  );
} finally {
  secrets.nightExternalKey.fill(0);
  secrets.zswapSeed.fill(0);
  secrets.dustSeed.fill(0);
}
```

Fetch normal wallet sync data from the adopter-selected indexer, then apply each
decoded batch contiguously:

```ts
await runtime.applySyncBatch(session, {
  stream: "shielded",
  fromOffset: 0,
  toOffset: 10,
  payloads: shieldedPayloads,
});

// Only after the indexer confirms offset 10 is its current tip:
await runtime.applySyncBatch(session, {
  stream: "shielded",
  fromOffset: 10,
  toOffset: 10,
  payloads: [],
});

const snapshot = await runtime.getWalletSnapshot(session);
```

Use the returned `streamOffsets` to request the next data. Applying a gap,
overlap, conflicting duplicate, or data for the wrong offset fails. Apply every
non-empty page contiguously. When the selected indexer confirms no more current
pages exist, apply exactly one terminal batch with an empty `payloads` array and
equal offsets at that stream's current offset.

The TypeScript/native boundary translates that public terminal convention into
the internal tip marker; applications continue to use only `shielded`,
`unshielded`, or `dust`. A later nonterminal data batch clears that stream's
caught-up state. The native runtime becomes `ready` only after all three streams
have received their terminal batch.

## Run and submit a transaction

The controller drives proof and balance effects until the command completes:

```ts
const transfer = await runtime.runCommand(
  session,
  {
    kind: "transfer",
    to: recipientAddress,
    amount: "5",
    tokenType,
    walletType: "shielded",
  },
  {
    signal: abortController.signal,
    onStep(step) {
      if (step.kind === "progress") {
        updateProgress(step.phase, step.percent);
      }
    },
  },
);

const submission = await runtime.runCommand(session, {
  kind: "submitFinalized",
  rawBase64: transfer.transactionBase64,
});
```

Decimal amounts are strings to avoid JavaScript precision loss. Do not retry an
operation concurrently on the same session. A submission transport failure is
ambiguous and throws `SUBMISSION_STATUS_UNKNOWN`; consult
`snapshot.pendingSubmissions` and reconcile through a trusted node or indexer
before taking another action.

## Close the runtime

Closing persists a final checkpoint when a store is configured, cancels active
work, clears SDK-owned native secrets, and invalidates the session handle:

```ts
await runtime.closeWalletSession(session);
await runtime.dispose();
```

For React applications, `MidnightRuntimeProvider` performs pause, refresh, and
dispose handling around application lifecycle changes. See the complete
[public API reference](./API_REFERENCE.md) and the repository's
[mock-first Expo example](../examples/expo/README.md).

## Before production evaluation

Complete these adopter-owned tasks:

1. Choose and authenticate indexer, proof, and node services.
2. Implement authenticated, encrypted, device-protected checkpoint storage.
3. Define seed acquisition, caller-buffer wiping, background, crash, and backup
   behavior.
4. Redact logs and crash reports.
5. Handle cancellation, stale sessions, sync gaps, proof failure, and ambiguous
   submission status.
6. Pin an exact SDK version and review its compatibility record, checksums,
   provenance, SBOM, license report, and release notes.

The [checkpoint contract](./CHECKPOINTS.md),
[seed threat model](./SEED_THREAT_MODEL.md), and
[alpha support policy](./ALPHA_SUPPORT.md) are required reading.
