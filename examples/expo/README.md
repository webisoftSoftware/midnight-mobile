# Expo SDK 55 mocked lifecycle

This clean Expo SDK 55 / React Native 0.83 example uses the public
`@1am/midnight-mobile` API. It runs against deterministic, non-routable mock
services by default and exercises:

- runtime creation and disposal;
- wallet open, sync progress, snapshots, and balances;
- in-memory checkpoint persistence and restoration;
- transaction construction, proving, and node submission;
- caller cancellation and a recoverable transport failure.

Build the repository package and its ignored native payloads, then generate a
clean development build:

```sh
# From the repository root, using Node.js 22 and the pinned npm:
npm ci
npm run build:native
npm run build:package

cd examples/expo
npx expo prebuild --clean
npx expo run:ios
npx expo run:android
```

The app cannot run in Expo Go because the SDK contains a custom native module.
After one platform development build is installed, `npm run ios` or
`npm run android` can start Metro and open it. Press **Run mocked lifecycle** in
the app.

`npm test` completes the same lifecycle for explicit `ios` and `android`
contexts. `npm run validate:platforms` exports each platform twice and verifies
that the emitted JavaScript bundle is byte-for-byte reproducible.

## Live preview mode

Mock mode never reads environment variables and cannot contact a real service.
Live preview mode is a separate, explicit integration point:
`createLivePreviewRuntime(configuration)`. Its `openWalletSession()` method
consumes the required wallet configuration and key buffers, then wipes the
caller-provided buffers on every outcome.

The caller must provide every indexer HTTP/WebSocket, proof-server, and node
URL; the endpoint-header function; HTTP and WebSocket implementations; wallet
configuration; and ephemeral wallet key material. The example contains no live
URL, API key, credential, or wallet key. Callers must wipe their original key
buffers after opening the wallet.

The included checkpoint adapter is memory-only. A live application must inject
authenticated, encrypted, device-protected persistence; this example does not
provide or select one.

Live preview is not enabled by an environment-variable switch and does not run
in required pull-request CI. Use disposable wallet material only, never
production or maintainer secrets. This example does not claim compatibility with
an endpoint merely because its URL is accepted.

## What to read next

- Follow the [clean-project quick start](../../docs/QUICK_START.md) to integrate
  the package outside this workspace.
- Read [architecture and resumable effects](../../docs/ARCHITECTURE.md) before
  replacing the mock services.
- Apply the [network configuration](../../docs/NETWORK_CONFIGURATION.md),
  [checkpoint encryption](../../docs/CHECKPOINTS.md), and
  [seed lifecycle](../../docs/SEED_THREAT_MODEL.md) contracts before any live
  preview evaluation.
- Check the [compatibility matrix](../../docs/COMPATIBILITY.md) and
  [alpha limitations](../../docs/ALPHA_SUPPORT.md).
