# Android embedded prover spike

This internal application exercises the SDK's Android local-prover bridge with
one deterministic `/check` request plus Zswap spend (k=15) and output (k=14)
`/prove` requests entirely on an arm64 Android device. It is not a second
implementation of the bridge. The application requests no Android permissions
and performs no network or filesystem access from Rust.

## Prepare and build

Use the repository's pinned Rust 1.97.1 toolchain, Android SDK platform 36, NDK
`27.1.12297006`, Gradle 9.0.0, Android Gradle Plugin 8.12.0, and JDK 17.

```sh
node tools/android-prover-spike/scripts/prepare-artifacts.mjs
GRADLE=/path/to/gradle-9.0.0/bin/gradle \
  node tools/android-prover-spike/scripts/build.mjs
```

The preparation script downloads eight public files from
`https://srs.midnight.network`, verifies their Ledger 8.1.0 SHA-256 hashes, and
stages them under the ignored `target/android-prover-spike/artifacts` directory.
No proving blob is committed. It also creates `request.bin`,
`output-request.bin`, and `check-request.bin` from the pinned Ledger
deterministic Zswap recipes and serializes the exact official endpoint tuples.

The build script compiles the same `midnight_mobile_runtime` library with the
`local-prover` feature, compiles the shared SDK `LocalProverBridge`, checks the
exact five local-prover C exports and unchanged eight UniFFI exports, and builds
a release arm64 APK. It fails unless all three requests and all eight artifacts
are stored uncompressed, the merged APK asks for no permission, and the APK
reports minSdk 24 and only `arm64-v8a` native code.

## Run on a physical device

Connect exactly one arm64 Android device, keep it on external power, and run:

```sh
node tools/android-prover-spike/scripts/run-device.mjs
```

The runner reinstalls the release APK, force-stops the old process, starts one
cold sequence, and samples PSS, RSS, `VmHWM`, and thread count until success,
process death, or 15 minutes. It records battery and thermal snapshots before
and after, pulls all three returned values, validates the `/check` result, and
requires both proofs to deserialize on the host as tagged `ProofVersioned::V2`.
Evidence is written to `target/android-prover-spike/device-run-report.json` even
when the app process dies after launch.

## Official server compatibility

Start the matching official server and send the identical prepared requests:

```sh
docker run --rm --name midnight-proof-server -p 6300:6300 \
  ghcr.io/midnight-ntwrk/proof-server:8.1.0
node tools/android-prover-spike/scripts/verify-official-server.mjs \
  http://127.0.0.1:6300
```

The compatibility script sends all three exact requests and writes both server
proofs only under `target/`.

## Buffer and timing semantics

Android opens each configured uncompressed artifact with `AssetManager.openFd`,
maps its region read-only, and passes direct native pointers through the same
JNA descriptors shipped by the SDK. Strong Kotlin references retain the mappings
until close. This avoids an artifact-sized `ByteArray`/UniFFI lowering copy. The
small endpoint request and response bodies do cross as byte arrays.

The probe logs configuration (mapping, integrity validation, and eager decode),
check, spend proving, and output proving timings separately. Each prove timer
includes upstream proof creation, mandatory built-in self-verification, and
tagged response serialization.
