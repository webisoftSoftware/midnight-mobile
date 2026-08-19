# Midnight Mobile Expo example

This Expo SDK 55 application demonstrates the public `@1am/midnight-mobile` API
with deterministic, non-routable mock services. It is useful for learning the
integration shape and for exercising the runtime in unit tests without
contacting an indexer or node.

The app demonstrates:

- creating a runtime provider and controller;
- opening a wallet session and clearing caller key buffers;
- applying ordered sync batches and reading wallet state;
- saving and restoring an in-memory checkpoint;
- creating, proving, and submitting a transaction through mock services;
- handling cancellation and a recoverable transport failure;
- loading the packaged native module in a development build; and
- running the SDK local prover's real `check` and `prove` operations.

The mock flow uses synthetic keys and addresses. It does not contain service
URLs, credentials, or a real wallet. The in-memory checkpoint store is for this
example only; production applications must provide authenticated encrypted
storage.

## Run it

From the repository root, use Node.js 22 and the pinned npm version:

```sh
npm ci
npm run build:native
npm run build:package

# Optional: download, verify, and stage the local-prover artifacts.
npm run prepare:local-prover --workspace @1am/midnight-mobile-example

cd examples/expo
npx expo prebuild --clean
npx expo run:ios
# or: npx expo run:android
```

The SDK includes a custom native module, so Expo Go is not supported. A
development build is required. After installing one, `npm run ios` or
`npm run android` starts Metro and opens the app.

Press **Run mocked lifecycle** to run the complete flow. Press **Run native
smoke test** to verify that the native runtime is present on the device or
simulator. Press **Run local prover** to perform a real native check and prove
using the staged spend circuit. The button reports a preflight failure if the
artifacts were not staged; it never substitutes a fake proof.

The app bundles only the four spend-circuit artifacts needed by this demo. The
artifact preparation command downloads the pinned public release files, checks
their size and SHA-256 values, and writes them to an ignored directory. Do not
commit those generated files. The deterministic check and prove requests are
small synthetic fixtures embedded in the example source.

## Test and validate

Run the deterministic mock tests and platform bundle check from this folder:

```sh
npm test
npm run validate:platforms
```

`npm test` compiles the current example sources and tests. It clears the ignored
TypeScript output first, so stale generated test files cannot be picked up by
the test glob. The unit tests verify platform-specific local-prover paths and
that a missing native module is reported as a failure.

`npm run validate:platforms` exports iOS and Android JavaScript bundles twice
and checks that each platform's output is reproducible. It does not contact a
live service.

## Optional live preview probe

The live preview sync probe is separate from local proving. Set these values in
a local, uncommitted `.env.local` file before starting the bundler:

```sh
EXPO_PUBLIC_MIDNIGHT_INDEXER_HTTP=https://your-indexer.example
EXPO_PUBLIC_MIDNIGHT_INDEXER_WS=wss://your-indexer.example/graphql
EXPO_PUBLIC_MIDNIGHT_NODE=https://your-node.example
```

The example does not validate that a service is safe or compatible merely
because its URL is accepted. Use disposable wallet material and credentials. The
live probe syncs indexer streams and checks node JSON-RPC. It does not run a
funded transfer: balance-service effects still require a separately configured
remote service, while `check` and `prove` use the local prover above.
