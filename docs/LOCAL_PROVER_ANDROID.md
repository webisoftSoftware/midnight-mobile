# Android local prover

The experimental Android local prover implements the Ledger 8.1.0 proof server's
binary `/check` and `/prove` behavior inside the SDK process. It is an explicit
package subpath and a separate Expo module; the root TypeScript API and the
runtime's eight-function UniFFI ABI are unchanged.

The same package subpath is available on iOS with platform-specific artifact
URIs and lifecycle behavior described in the
[iOS local prover guide](LOCAL_PROVER_IOS.md).

## Configure and use

```ts
import {
  createLocalProverMidnightTransport,
  createMidnightLocalProver,
} from "@1am/midnight-mobile/local-prover";

const prover = await createMidnightLocalProver({
  parameters: [
    {
      k: 15,
      file: {
        uri: "asset://midnight/bls_midnight_2p15",
        size: parameterSize,
        sha256: parameterSha256,
      },
    },
  ],
  circuits: [
    {
      keyLocation: "midnight/zswap/spend",
      proverKey: proverKeyFile,
      verifierKey: verifierKeyFile,
      ir: irFile,
    },
  ],
});

const checked = await prover.check(ledgerCheckRequest);
const proof = await prover.prove(ledgerProveRequest);

const transport = createLocalProverMidnightTransport(
  standardTransportConfiguration,
  prover,
);

await prover.close();
```

Each file descriptor requires its exact byte size and lowercase SHA-256. Android
accepts `asset://` paths for uncompressed APK assets and absolute paths inside
the application's sandbox. Relative paths, Android `bundle://` paths, compressed
assets, size mismatches, and hash mismatches fail configuration. Applications
must package or provision the artifacts themselves; the SDK never downloads
them.

Configuration maps each file read-only with `FileChannel`, passes the direct
mapping to Rust, verifies its digest, and eagerly decodes the parameters, IR,
prover key, and verifier key. Parameter intrinsic `k`, prover-key `k`, and IR
`k` are checked before the new registry generation becomes active. Ledger does
not expose the initialized verifier-key `k` publicly, so the verifier key is
eagerly initialized and remains subject to the upstream proof self-verification.
Mappings remain strongly referenced until `close()`.

Circuit descriptors form a generic key-location registry and parameter
descriptors form a generic `k` registry. A `/check` request carrying inline
`WrappedIr` does not need a matching circuit descriptor. A `/prove` request
carrying `ProvingKeyMaterial` also does not need one, but proving always
requires configured parameters matching the material's `k`.

## Transport and execution boundaries

`createLocalProverMidnightTransport` intercepts only proof-role `check` and
`prove` effects. `proveAndBalance`, `balance`, submission, indexer, and all
other effects keep using the configured remote transport. Local fee balancing
and remote fallback after a local proof error are intentionally not implemented.

The Android Expo async bridge runs outside the main thread. Rust serializes
configuration, checking, proving, and close through one process-wide permit and
runs upstream proving in a fixed two-thread Rayon pool. A second bridge caller
receives `PROVER_BUSY`. Upstream proving is not cooperatively cancellable, so
this API exposes no cancellation contract. Call `close()` after outstanding
operations settle; it is idempotent at the TypeScript service boundary.

Stable failures are `INVALID_REQUEST`, `UNSUPPORTED_CIRCUIT`,
`INTEGRITY_CHECK_FAILED`, `PROVER_BUSY`, `RESOURCE_PREFLIGHT_FAILED`,
`PROOF_FAILED`, `INVALID_CONFIGURATION`, `STALE_REGISTRY`, `CHECK_FAILED`, and
`NATIVE_INTERNAL`.

## Native packaging policy

Android release libraries are built with the private Cargo `local-prover`
feature and expose five reviewed C symbols beside the unchanged eight UniFFI
functions. The feature-enabled shipping libraries remain subject to the normal
30 MiB combined Android native-payload budget. Proof parameters and circuit
artifacts are application content and are never included in the SDK package or
native-size budget.

See [the device spike report](ANDROID_EMBEDDED_PROVER_SPIKE.md) and
[`tools/android-prover-spike`](../tools/android-prover-spike/README.md) for the
offline k=15 Zswap acceptance workflow.
