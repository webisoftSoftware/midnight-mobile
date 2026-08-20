# Midnight Mobile

Midnight Mobile is an experimental wallet runtime for React Native and Expo. It
is not an official Midnight SDK. APIs and data formats may change between
releases.

## What it does

Midnight Mobile can:

- Open a wallet session with keys supplied by the application.
- Apply wallet updates from an indexer.
- Create, prove, and submit wallet transactions.
- Save and restore wallet checkpoints.
- Connect the wallet runtime to the React application lifecycle.
- Run the experimental `check` and `prove` operations on the device.

The Rust runtime never accesses the network directly. The application must
provide the indexer, proof server, node, HTTP and WebSocket clients, request
headers, and checkpoint storage.

## What it does not do

Midnight Mobile does not:

- Create or store a mnemonic.
- Derive application keys or addresses.
- Operate an indexer, proof server, or node.
- Supply service URLs or credentials.
- Provide secure persistent storage.
- Download proof files.
- Support Expo Go.

## Supported versions

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

The runtime accepts the network IDs `preview`, `preprod`, and `mainnet`. A valid
network ID does not prove that a service is safe or compatible. Test every
service before using it with a wallet.

## Install in an Expo app

Use an Expo 55 development build.

```sh
npm install --save-exact @1am/midnight-mobile@0.1.0-alpha.1
npx expo prebuild --clean
npx expo run:ios
npx expo run:android
```

For local development, install an npm package file that has passed the package
checks. Do not install native libraries or generated bindings separately.

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

Use HTTP or HTTPS URLs for HTTP services and WS or WSS for the indexer
WebSocket. Use TLS for every remote service. Never log service URLs, headers,
credentials, keys, checkpoints, or transaction data.

## Open and sync a wallet session

Each key must contain exactly 32 bytes. The runtime clears its own copies. The
application must clear the original arrays.

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

Pass every amount as a decimal string to avoid JavaScript number-precision
errors.

Close the session when work is complete.

```ts
await runtime.closeWalletSession(session);
await runtime.dispose();
```

## Balance a transaction from a dApp

A dApp can provide a transaction that it cannot fund. `previewBalance` reports
the tokens and DUST that the wallet must provide. The command does not change
wallet state. It does not create valid signatures or proofs. It selects inputs
from a copy of the wallet state to calculate the manifest. Show the manifest to
the user. After approval, give the same manifest to `balanceUnsealed` or
`balanceSealed`.

```ts
const preview = await runtime.runCommand(session, {
  kind: "previewBalance",
  rawBase64: transactionFromDapp,
  sealed: false,
  ledgerParametersBase64,
  feeBlocksMargin: 5,
  additionalFeeOverhead: "0",
  feeMode: "localDust",
});

// preview.manifest.contributions contains the wallet debit for each token.
// The debit is the selected value minus the returned change.
// preview.manifest.dust contains the maximum DUST that the wallet can spend.
// Its value is "sponsored" if the transaction does not pay this fee.
if (!(await confirmWithUser(preview.manifest))) return;

const balanced = await runtime.runCommand(session, {
  kind: "balanceUnsealed",
  rawBase64: transactionFromDapp,
  ledgerParametersBase64,
  feeBlocksMargin: 5,
  additionalFeeOverhead: "0",
  feeMode: "localDust",
  approvedManifest: preview.manifest,
});
```

The runtime makes the plan again before execution. It returns
`BALANCE_APPROVAL_CHANGED` if the transaction, token contributions, or wallet
coins do not match the approved manifest. The DUST cost can be lower than the
approved cost. It cannot be higher.

The runtime changes an unsealed transaction to balance it. This change also
changes the signing data for all parts of the intent. The runtime signs all inputs
that the wallet owns in both offer sections. It rejects an input that another
party owns.

The runtime does not change a sealed transaction. It balances the transaction
with a separate intent. Some sealed transactions cannot use this method. The
ledger binds a fallible section to its own intent. The runtime returns
`UNSUPPORTED_TRANSACTION` for these transactions. It returns
`INSUFFICIENT_FUNDS` for a token shortage. It does not report a token shortage
as `INSUFFICIENT_DUST`.

## Network results

The runtime does not retry a failed submission.

| Result                                 | Submission     | Other network operation |
| -------------------------------------- | -------------- | ----------------------- |
| HTTP 2xx                               | accepted       | accepted                |
| HTTP 4xx                               | rejected       | rejected                |
| HTTP 3xx, HTTP 5xx, or transport error | status unknown | transport error         |
| Caller cancellation                    | cancelled      | cancelled               |

If a submission has an unknown status, check the transaction with a trusted node
or indexer. Do not submit the same transaction until you know its status.

## Protect checkpoints and keys

A checkpoint contains private wallet data. Use the in-memory checkpoint store
only in tests and examples. A production checkpoint store must:

- Encrypt and authenticate its data.
- Replace existing data safely.
- Prevent rollback to an older checkpoint.
- Delete data when the application requests deletion.

JavaScript cannot guarantee immediate memory removal. Keep key arrays for the
shortest possible time. Do not convert keys to strings. Do not put keys in logs,
telemetry, or crash reports.

Checkpoint migration between releases is not guaranteed.

## Use the local prover

The `@1am/midnight-mobile/local-prover` entry point runs the `check` and `prove`
proof operations locally on iOS and Android.

The application supplies all proof parameters and circuit files. Each file must
have an expected size and SHA-256 value. The runtime opens each file in
read-only mode. It does not download or write these files.

Only `check` and `prove` run locally. Other network operations still use the
configured remote services. Active local proof work cannot be cancelled. A
concurrent request may return `PROVER_BUSY`.

The Android local prover accepts an uncompressed `asset://` file or an absolute
path in the application sandbox. The iOS local prover accepts a `bundle://` file
or an absolute path in the application sandbox.

## Run the example

The `examples/expo` application is a one-button demonstration with deterministic
mock host services and real native runtime and prover checks. It does not
contain a real service credential or mnemonic. See the
[example guide](./examples/expo/README.md) for setup and the scope boundary
between mock host effects and on-device proving.

```sh
npm ci
npm run build:native
npm run build:package
cd examples/expo
npx expo prebuild --clean
```

Use disposable keys for tests that contact a network.

## Repository layout

Each top-level directory has one main purpose. Generated native bindings stay
beside the package that publishes them. Build outputs are ignored.

| Path                     | Responsibility                                                |
| ------------------------ | ------------------------------------------------------------- |
| `crates/runtime/`        | Rust wallet runtime and local-prover native interfaces        |
| `packages/react-native/` | Public TypeScript API and iOS/Android bridges                 |
| `examples/expo/`         | Minimal Expo consumer and deterministic host mocks            |
| `scripts/`               | Required quality, packaging, security, and release automation |
| `tools/`                 | Focused binding, device, simulator, and measurement utilities |

Runtime command handlers, operation resume handlers, and wallet-state code are
regular Rust modules under `crates/runtime/src/`. See
[`scripts/README.md`](./scripts/README.md) and
[`tools/README.md`](./tools/README.md) for the boundary between automation and
manual developer utilities.

## Validate a change

Use Node.js 22 and the npm version in `package.json`.

```sh
npm ci
npm run quality:pr
```

Pull requests run formatting, linting, type checks, unit tests, and the package
source check.

A release-candidate tag runs the full native and release checks. These checks
verify the native application binary interface (ABI), package contents, clean
consumer builds, security rules, and reproducible builds. The workflow also
creates checksums, a software bill of materials (SBOM), a provenance record, and
a dependency license report.

| Command                        | Function                                                   |
| ------------------------------ | ---------------------------------------------------------- |
| `npm test`                     | Run tests without coverage.                                |
| `npm run test:coverage`        | Run tests and enforce coverage thresholds.                 |
| `npm run typecheck`            | Check TypeScript types.                                    |
| `npm run check:package:source` | Check package metadata and reproducible TypeScript output. |
| `npm run check:bindings`       | Check the native interface and generated bindings.         |
| `npm run check:package`        | Check all files in the native npm package.                 |
| `npm run check:native`         | Check native archives and consumer builds.                 |
| `npm run quality`              | Run the complete local gate.                               |
| `npm run quality:rc`           | Run the same complete gate explicitly.                     |

## Support and security

Support is limited to the latest prerelease and the tested versions in this
file. The project provides no production support guarantee.

Read [SECURITY.md](./SECURITY.md) before reporting a security problem. Do not
put private wallet data in a public report.

## License

The project uses the [MIT License](./LICENSE). Third-party software keeps its
own license. See [NOTICE](./NOTICE) for required notices.
