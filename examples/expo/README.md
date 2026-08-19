# Midnight Mobile Expo example

This small Expo 55 application is a guided SDK tour. Press **Run SDK example**
once to see three compact phases:

- **Mock host flow** opens the public test wallet, applies all three sync
  streams, reads balances, creates and submits a transaction, and restores a
  checkpoint.
- **Native runtime** loads the packaged Rust runtime and runs session,
  checkpoint, and signing operations on the device or simulator.
- **Native prover** runs the real local `check` and `prove` operations against
  the staged spend circuit. No proof server is contacted for these operations.

The host flow is deterministic and never contacts an indexer, proof service, or
node. It uses the SDK's mock-native module to make the controller flow easy to
read and repeat. The native phases are the device-facing checks that require a
development build.

The displayed wallet address is a deterministic public preview fixture derived
from the synthetic key bytes in `src/demo-wallet.ts`. It is not generated from
or accompanied by a mnemonic. Never fund or reuse it. A real application owns
key management and must clear its original key arrays after opening a session.

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

The SDK includes a custom native module, so Expo Go is not supported. Use a
development build. The prover artifacts are downloaded from the pinned public
ledger release, checked by size and SHA-256, and copied into an ignored
directory. They are intentionally not committed to the example.

If the artifacts are not staged, the host and native-runtime phases still show
their results and the native-prover phase reports the missing-artifact error.
The example never substitutes a fake proof response.

## Test it

From this folder:

```sh
npm run typecheck
npm test
npm run validate:platforms
```

The tests use deterministic synthetic fixtures only. `validate:platforms`
exports reproducible iOS and Android JavaScript bundles; it does not contact a
live service.

## Scope boundary

The controller transaction is a deterministic host-boundary simulation. Its
network effects use mock services; the separate native-prover phase runs the
real `check` and `prove` operations. In production, balance and submission still
use application-supplied host services. This example is safe to run without
credentials or a funded wallet.
