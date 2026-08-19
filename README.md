# Midnight Mobile

Midnight Mobile is an experimental wallet runtime for React Native and Expo. It
is not an official Midnight SDK. The API and data formats can change in every
release.

## Functions

The SDK provides these functions:

- The SDK opens a wallet session with application keys.
- The SDK applies wallet data from an indexer.
- The SDK creates and proves wallet transactions.
- The SDK submits finalized transactions to a node.
- The SDK saves and restores wallet checkpoints.
- The SDK connects the wallet runtime to the React application lifecycle.
- The SDK runs the experimental proof operations `check` and `prove` on a
  device.

The Rust runtime does not access the network. The application supplies the
indexer, proof server, node, HTTP client, WebSocket client, headers, and
checkpoint store.

## Limits

The SDK has these limits:

- The SDK does not create or store a mnemonic.
- The SDK does not derive application keys or addresses.
- The SDK does not operate an indexer, proof server, or node.
- The SDK does not supply service URLs or credentials.
- The SDK does not provide secure persistent storage.
- The SDK does not download proof files.
- The SDK does not support Expo Go.
- The SDK has no production support.

## Compatibility

Use the versions in this table. Other versions are not supported.

| Component         | Tested version or range                                |
| ----------------- | ------------------------------------------------------ |
| npm package       | `@1am/midnight-mobile@0.1.0-alpha.1`                   |
| Expo              | `55.0.28` (`>=55 <56`)                                 |
| Expo Modules Core | `55.0.25` (`>=55 <56`)                                 |
| React             | `19.2.0` (`>=19.2 <20`)                                |
| React Native      | `0.83.6` (`>=0.83 <0.84`)                              |
| Node.js           | 22 or 24                                               |
| iOS               | 15.1 or later; arm64 device; arm64 or x86_64 simulator |
| Android           | API 24 or later; `arm64-v8a` or `x86_64`               |
| Midnight Ledger   | 8.1.0 at `02716c2c95d50654aeb3cb63bfd8386046e4ca7d`    |

The SDK accepts the network IDs `preview`, `preprod`, and `mainnet`. A network
ID does not confirm that a service is safe or compatible. Test each service
before use.

## Install

Use an Expo 55 development build.

```sh
npm install --save-exact @1am/midnight-mobile@0.1.0-alpha.1
npx expo prebuild --clean
npx expo run:ios
npx expo run:android
```

For a local build, install an npm package file that passes the package checks.
Do not install the native libraries or generated bindings as separate packages.

## Create the runtime

The application must supply all host services.

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
declare const fetchFromHost: MidnightFetch;
declare const createWebSocket: MidnightWebSocketFactory;
declare const serviceConfig: {
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
    indexerHttpUrl: serviceConfig.indexerHttpUrl,
    indexerWebSocketUrl: serviceConfig.indexerWebSocketUrl,
    proofServerUrl: serviceConfig.proofServerUrl,
    nodeUrl: serviceConfig.nodeUrl,
  },
  fetch: fetchFromHost,
  createWebSocket,
  headers: serviceConfig.headersFor,
});

const runtime = new MidnightRuntimeController({
  api: createMidnightRuntimeApi(),
  transport,
  checkpointStore,
});
```

Use HTTP or HTTPS service URLs. Use WS or WSS for the indexer WebSocket URL. Use
TLS with remote services. Do not log URLs, headers, credentials, keys,
checkpoints, or transaction data.

## Open a wallet session

Each key input must contain exactly 32 bytes. The SDK clears only its own key
copies. The application must clear the original arrays.

```ts
const keys = {
  nightExternalKey: obtainNightExternalKey(),
  zswapSeed: obtainZswapSeed(),
  dustSeed: obtainDustSeed(),
};

let session;
try {
  session = await runtime.openWalletSession(
    {
      networkId: "preview",
      walletFingerprint: "stable-non-secret-wallet-id",
      unshieldedAddress: "application-derived-address",
    },
    keys,
  );
} finally {
  keys.nightExternalKey.fill(0);
  keys.zswapSeed.fill(0);
  keys.dustSeed.fill(0);
}
```

Use a non-secret value for `walletFingerprint`. Do not use key data for this
value.

Apply indexer batches in order. A gap, overlap, or wrong offset causes an error.
Send one empty batch when a stream reaches the current indexer tip.

```ts
await runtime.applySyncBatch(session, {
  stream: "shielded",
  fromOffset: 0,
  toOffset: 10,
  payloads: shieldedPayloads,
});

await runtime.applySyncBatch(session, {
  stream: "shielded",
  fromOffset: 10,
  toOffset: 10,
  payloads: [],
});
```

The wallet is ready after the `shielded`, `unshielded`, and `dust` streams reach
their current tips.

Run one command at a time for each session.

```ts
const transfer = await runtime.runCommand(session, {
  kind: "transfer",
  to: recipientAddress,
  amount: "5",
  tokenType,
  walletType: "shielded",
});

await runtime.runCommand(session, {
  kind: "submitFinalized",
  rawBase64: transfer.transactionBase64,
});
```

Pass each amount as a decimal string. A decimal string prevents number precision
errors.

Close the session when work is complete.

```ts
await runtime.closeWalletSession(session);
await runtime.dispose();
```

## Network results

The SDK does not retry a failed submission.

| Result                                 | Submission     | Other network operation |
| -------------------------------------- | -------------- | ----------------------- |
| HTTP 2xx                               | accepted       | accepted                |
| HTTP 4xx                               | rejected       | rejected                |
| HTTP 3xx, HTTP 5xx, or transport error | status unknown | transport error         |
| Caller cancellation                    | cancelled      | cancelled               |

If a submission has an unknown status, check the transaction with a trusted node
or indexer. Do not submit the same transaction until you know its status.

## Protect checkpoints and keys

A checkpoint contains private wallet data. The in-memory checkpoint store is for
tests and examples only. A real application must use encrypted storage. The
storage must authenticate the data. It must replace data safely. It must prevent
rollback attacks. It must also delete data when the application requests it.

JavaScript cannot guarantee immediate memory removal. Keep key arrays for the
shortest possible time. Do not convert keys to strings. Do not put keys in logs,
telemetry, or crash reports.

Checkpoint migration between releases is not guaranteed.

## Use the local prover

The `@1am/midnight-mobile/local-prover` entry point runs the `check` and `prove`
proof operations on iOS and Android.

The application supplies all proof parameters and circuit files. Each file must
have an expected size and SHA-256 value. The SDK opens each file in read-only
mode. The SDK does not download or write these files.

The local prover handles only `check` and `prove`. Other operations use the
configured remote services. The local prover cannot cancel active proof work. It
can return `PROVER_BUSY` when another proof operation is active.

The Android local prover accepts an uncompressed `asset://` file or an absolute
path in the application sandbox. The iOS local prover accepts a `bundle://` file
or an absolute path in the application sandbox.

## Run the example

The `examples/expo` application runs a wallet flow with local test services. It
does not contain a real service URL, credential, or wallet key.

```sh
npm ci
npm run build:native
npm run build:package
cd examples/expo
npx expo prebuild --clean
```

Use disposable keys for tests that contact a network.

## Validate a change

Use Node.js 22 and the npm version in `package.json`.

```sh
npm ci
npm run quality:pr
```

Pull requests run format checks, lint checks, type checks, unit tests, and the
package source check.

A release candidate (RC) tag runs native builds and release checks. These checks
verify the native ABI, package contents, clean consumer builds, security rules,
and build reproducibility. The RC workflow also creates checksums, a software
bill of materials (SBOM), a provenance record, and a dependency license report.

| Command                        | Function                                                   |
| ------------------------------ | ---------------------------------------------------------- |
| `npm test`                     | Run tests without coverage.                                |
| `npm run test:coverage`        | Run tests with coverage for an RC.                         |
| `npm run typecheck`            | Check TypeScript types.                                    |
| `npm run check:package:source` | Check package metadata and reproducible TypeScript output. |
| `npm run check:bindings`       | Check the native ABI and generated bindings.               |
| `npm run check:package`        | Check all files in the native npm package.                 |
| `npm run check:native`         | Check native archives and consumer builds.                 |
| `npm run quality:rc`           | Run all RC checks.                                         |

## Support and security

Support is limited to the latest prerelease and the tested versions in this
file. The project provides no production support guarantee.

Read [SECURITY.md](./SECURITY.md) before reporting a security problem. Do not
put private wallet data in a public report.

## License

The project uses the [MIT License](./LICENSE). Third-party software keeps its
own license. See [NOTICE](./NOTICE) for required notices.
