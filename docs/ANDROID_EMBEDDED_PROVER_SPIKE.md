# Android embedded Rust prover feasibility report

## Decision

Status: **successful on the tested device**.

The cold-process proof completed well within 15 minutes, the upstream proving
operation self-verified it, the Android process remained alive, and the returned
bytes deserialized on the host as tagged `ProofVersioned::V2`.

## Pinned inputs

| Input                    |      Bytes | SHA-256                                                            |
| ------------------------ | ---------: | ------------------------------------------------------------------ |
| `bls_midnight_2p15`      |  6,291,844 | `724c7c3d779148bb113c7ee9c034b2f27db16e6bdf315fde90105a9bad00b1de` |
| `zswap/9/spend.prover`   | 11,020,001 | `19d234b5c68b7212ad6b0ec9334a95594748154128f3704eb576bcc843cc5c45` |
| `zswap/9/spend.verifier` |      2,311 | `544554effd7ae9fb9063be52a9ec2a986756301071fcd97bb4598fb45a335658` |
| `zswap/9/spend.bzkir`    |      1,294 | `7cb5bbcf67cb212a3336fb439a77e8f32f0aa8a56185c8e1247d6cbfc7300205` |

Source: `https://srs.midnight.network/`. Ledger Git revision:
`02716c2c95d50654aeb3cb63bfd8386046e4ca7d`.

The deterministic 578-byte `/prove` request has SHA-256
`9cfed5466ae1dae73dab40373c81abf94ba15ee1c6c1c3f004156cdb0a61f608`.

## Tested toolchain

| Component                      | Tested version                                         |
| ------------------------------ | ------------------------------------------------------ |
| Rust                           | 1.97.1                                                 |
| Android NDK / native API       | 27.1.12297006 / 24                                     |
| Android platform / build tools | 36 / 36.0.0                                            |
| Gradle / Android Gradle Plugin | 9.0.0 / 8.12.0                                         |
| Kotlin Gradle plugin / JDK     | 2.1.20 / 17.0.19                                       |
| Node.js                        | 24.15.0 (repository tooling remains pinned to Node 22) |

## Reproduction

```sh
node tools/android-prover-spike/scripts/prepare-artifacts.mjs
MIDNIGHT_ANDROID_PROVER_ARTIFACT_DIR="$PWD/target/android-prover-spike/artifacts" \
  cargo test --offline --locked --release \
  --package midnight-native-runtime --features local-prover \
  local_prover::tests::staged_artifacts_produce_and_check_official_responses \
  -- --ignored --exact --nocapture
GRADLE=/path/to/gradle-9.0.0/bin/gradle \
  node tools/android-prover-spike/scripts/build.mjs
node tools/android-prover-spike/scripts/run-device.mjs
```

Build sizes and dependency/packaging inspection are recorded in
`target/android-prover-spike/android-build-report.json`. Device identity,
battery, thermal snapshots, memory samples, `VmHWM`, wall time, thread peak,
process survival, and the final result line are recorded in
`target/android-prover-spike/device-run-report.json`.

## Build result

| Item                            |      Bytes | SHA-256                                                            |
| ------------------------------- | ---------: | ------------------------------------------------------------------ |
| `libmidnight_native_runtime.so` | 12,984,784 | `81fe02c2127d715fd06f02dc0b4830688c3cf4e3f3cc844907bcab7c437fc151` |
| release APK                     | 33,197,380 | `0c1b0a9a62907f21795ae8c30273ab11557c19313f0e13024786b09a6e110e50` |
| four proof artifacts            | 17,315,450 | See pinned-input table                                             |

The native library targets `aarch64-linux-android` at API 24 and links only
`libc.so`, `libdl.so`, and `libm.so`. The APK has minSdk 24, targetSdk 36,
contains only `arm64-v8a`, asks for no permissions, and stores both requests and
all four proof artifacts uncompressed. The physical run used the shared SDK
Kotlin bridge and production-shaped Rust C ABI for both `/check` and `/prove`.

## Physical-device result

| Metric                         | Result                                                                                                                  |
| ------------------------------ | ----------------------------------------------------------------------------------------------------------------------- |
| Device                         | Samsung SM-G973W                                                                                                        |
| OS / ABI                       | Android 12, API 31, `arm64-v8a`                                                                                         |
| RAM                            | 7,640,540 KiB `MemTotal`                                                                                                |
| Online CPU count               | 6 before and after the measured run                                                                                     |
| Rust Rayon configuration       | Fixed two-thread pool                                                                                                   |
| Battery                        | 100%; 27.8°C to 27.9°C; USB powered                                                                                     |
| Thermal                        | Status 0 before and after; AP 30.5°C to 33.5°C                                                                          |
| Artifact validation/decoding   | 16,272 ms                                                                                                               |
| `/check`                       | 31 ms; 47 bytes; SHA-256 `18969d942d685cae2838d76a85199eb7f488b43ccba5ba8f4542120459451b5a`; host validation passed     |
| Proving plus self-verification | 37,869 ms                                                                                                               |
| Runner wall time               | 57,658 ms                                                                                                               |
| Tagged proof                   | 4,860 bytes; SHA-256 `e88bb29157d6d52b9f1ca4b618a32f46c53e142bedfaba1615444502c915888e`; host V2 deserialization passed |
| Peak PSS                       | 529,419 KiB                                                                                                             |
| Peak RSS                       | 619,652 KiB                                                                                                             |
| Peak `VmHWM`                   | 613,848 KiB                                                                                                             |
| Peak observed process threads  | 31 total Android/runtime threads                                                                                        |
| Process result                 | Alive after success; no OOM or native crash                                                                             |

The fixed Rayon limit applies to proof parallelism, not to Android, JNA, Rust,
and runtime support threads included in the observed total thread count.

The identical request was also accepted by the official Ledger 8.1.0 proof
server. Its 4,860-byte response deserialized on the host as tagged V2.

## Scope and recommendation rule

This experiment covers only the largest built-in wallet circuit requested for
the spike: Zswap spend at `k=15`, on one arm64 device. It does not establish
support for every API-24-era phone and does not propose a production SDK API.

Recommendation: **go for production design work, not production shipment**. The
measured run reports `success: true`, so this spike's feasibility gate is
satisfied on the tested phone. A production implementation still needs live
memory admission, cache-lifetime and secure-artifact handling, broader device
qualification, UX requirements for roughly 38-second proof latency, and a
deliberate cancellation/process-isolation design. The result does not establish
support for every API-24-era device.
