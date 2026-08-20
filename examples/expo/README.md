# Midnight Mobile Expo example

This Expo 55 application demonstrates Midnight Mobile in three phases. Press
**Run SDK example** once to run all three:

- **Mock host flow:** Opens a test wallet, syncs all three streams, reads its
  balances, creates and submits a transaction, and restores a checkpoint.
- **Native runtime:** Loads the packaged Rust runtime and runs session,
  checkpoint, and signing operations on the device or simulator.
- **Native prover:** Runs the real local `check` and `prove` operations with the
  staged spend circuit. These operations do not contact a proof server.

The host flow is deterministic. It uses mock services and never contacts an
indexer, proof service, or node. The two native phases require a development
build and run on the device or simulator.

The displayed address is a public test fixture derived from the synthetic keys
in `src/demo-wallet.ts`. It has no mnemonic. Never fund or reuse this address. A
real application must manage its own keys and clear the original key arrays
after opening a session.

## Run it

From the repository root, use Node.js 22 and the pinned npm version:

```sh
npm ci
npm run build:native
npm run build:package

# Download, verify, and stage the public local-prover artifacts.
npm run prepare:local-prover --workspace @1am/midnight-mobile-example

cd examples/expo
npx expo prebuild --clean
npx expo run:ios
# or: npx expo run:android
```

Midnight Mobile includes native code, so Expo Go is not supported. The
preparation command downloads prover files from the pinned public ledger
release, verifies their size and SHA-256 hash, and copies them into an ignored
directory. These files are not committed.

If the prover files are missing, the host and native-runtime phases still run.
The native-prover phase reports the missing files; it never returns a fake proof
response.

## Test it

From this folder:

```sh
npm run typecheck
npm test
npm run validate:platforms
```

Tests use deterministic synthetic data only. `validate:platforms` exports
reproducible iOS and Android JavaScript bundles without contacting a live
service.

## What is real and what is mocked

The controller transaction uses mock network services. The native-prover phase
runs the real `check` and `prove` operations on the device. Production balance
and submission operations require services supplied by the application. You can
run this example without credentials or a funded wallet.
