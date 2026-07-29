# Expo SDK 55 mocked lifecycle

This clean Expo SDK 55 / React Native 0.83 example uses the public
`@1am/midnight-mobile` API. It runs against deterministic, non-routable mock
services by default and exercises:

- runtime creation and disposal;
- wallet open, sync progress, snapshots, and balances;
- in-memory checkpoint persistence and restoration;
- transaction construction, proving, and node submission;
- caller cancellation and a recoverable transport failure.

Run `npm run ios` or `npm run android` from this directory after the repository
dependencies and package build are complete. Press **Run mocked lifecycle** in
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
