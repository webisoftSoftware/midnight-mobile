# Android embedded prover spike

This internal application proves one deterministic Zswap spend entirely on an
arm64 Android device. It is not part of the npm package or the supported SDK
API. The application requests no Android permissions and performs no network or
filesystem access from Rust.

## Prepare and build

Use the repository's pinned Rust 1.97.1 toolchain, Android SDK platform 36, NDK
`27.1.12297006`, Gradle 9.0.0, Android Gradle Plugin 8.12.0, and JDK 17.

```sh
node tools/android-prover-spike/scripts/prepare-artifacts.mjs
GRADLE=/path/to/gradle-9.0.0/bin/gradle \
  node tools/android-prover-spike/scripts/build.mjs
```

The preparation script downloads four public files from
`https://srs.midnight.network`, verifies their Ledger 8.1.0 SHA-256 hashes, and
stages them under the ignored `target/android-prover-spike/artifacts` directory.
No proving blob is committed. It also creates `request.bin` from the pinned
Ledger deterministic Zswap spend recipe and serializes the exact official
`/prove` tuple.

The build script compiles the same `midnight_native_runtime` library with the
`android-prover-spike` feature, generates Kotlin bindings only under `target/`,
checks ELF dependencies and spike symbols, and builds a release arm64 APK. It
fails unless all five input files are stored uncompressed, the merged APK asks
for no permission, and the APK reports minSdk 24 and only `arm64-v8a` native
code.

## Run on a physical device

Connect exactly one arm64 Android device, keep it on external power, and run:

```sh
node tools/android-prover-spike/scripts/run-device.mjs
```

The runner reinstalls the release APK, force-stops the old process, starts one
cold proof, and samples PSS, RSS, `VmHWM`, and thread count until success,
process death, or 15 minutes. It records battery and thermal snapshots before
and after, pulls the returned proof, and requires host deserialization as tagged
`ProofVersioned::V2`. Evidence is written to
`target/android-prover-spike/device-run-report.json` even when the app process
dies after launch.

## Official server compatibility

Start the matching official server and send the identical prepared request:

```sh
docker run --rm --name midnight-proof-server -p 6300:6300 \
  ghcr.io/midnight-ntwrk/proof-server:8.1.0
node tools/android-prover-spike/scripts/verify-official-server.mjs \
  http://127.0.0.1:6300
```

The compatibility script writes the server response only under `target/`.

## Buffer and timing semantics

Android opens each uncompressed asset with `AssetManager.openFd`, maps its
region read-only, and passes direct native pointers through the spike-only JNA
ABI. A strong Kotlin reference to all mappings is retained for the complete
synchronous call. This avoids the additional `ByteArray`/UniFFI lowering copy.
The upstream `ProofPreimage::prove` resolver requires owned
`ProvingKeyMaterial`, so Rust makes one owned key/IR allocation after mapping.

`artifactDecodingMillis` includes SHA-256 verification, IR and parameter
parsing, and eager prover/verifier-key initialization. The upstream global
prover-key cache is then reused by the prove call. The second timer covers proof
creation and its mandatory built-in self-verification. Proof serialization is
excluded from both timers.
