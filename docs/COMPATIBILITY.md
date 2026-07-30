# Compatibility matrix

This record applies to the source candidate for
`@1am/midnight-mobile@0.1.0-alpha.1`. No package has been published yet. A
release is supported only when its tag, package version, binaries, and release
evidence agree.

## JavaScript and Expo

| Layer             | Exact tested version                          | Declared support                                        | Not supported or not tested                      |
| ----------------- | --------------------------------------------- | ------------------------------------------------------- | ------------------------------------------------ |
| Node.js           | `22.x` repository engine                      | Node.js 22 for install/build tooling                    | Node 21 or earlier; Node 23 or later             |
| npm               | `11.12.1`                                     | Exact lockfile/package-manager version for reproduction | Yarn, pnpm, Bun, or a regenerated lockfile       |
| Expo              | `55.0.28`                                     | `>=55.0.0 <56.0.0`                                      | Expo Go; SDK 54 or 56+                           |
| Expo Modules Core | `55.0.25`                                     | `>=55.0.0 <56.0.0`                                      | Other major SDK lines                            |
| React             | `19.2.0`                                      | `>=19.2.0 <20.0.0`                                      | React 18 or 20+                                  |
| React Native      | `0.83.6`                                      | `>=0.83.0 <0.84.0`                                      | React Native 0.82 or 0.84+                       |
| TypeScript        | `5.9.3`                                       | Emitted declarations validated with this compiler       | Older compiler compatibility is not promised     |
| Module format     | ECMAScript modules                            | Package-root ESM import                                 | CommonJS `require`, browser, SSR, or web runtime |
| RN architecture   | Expo 55 default with New Architecture enabled | Clean Expo development build                            | Legacy Architecture is not release-tested        |

The peer ranges state the only candidate compatibility window. The exact
versions are the release-tested set. A combination inside the ranges is not
supported merely because npm accepts it; reproduce with the exact set before
reporting a compatibility defect.

## Apple platforms

| Property                | Supported/tested value                                                                           |
| ----------------------- | ------------------------------------------------------------------------------------------------ |
| Minimum OS              | iOS `15.1`                                                                                       |
| Device architecture     | `arm64`                                                                                          |
| Simulator architectures | `arm64`, `x86_64`                                                                                |
| Build product           | Dynamic `MidnightNativeRuntime.xcframework` embedded by CocoaPods                                |
| Swift setting           | Swift `5.9` in the package podspec                                                               |
| M5 release baseline     | Xcode `16.4`, CocoaPods `1.16.2`                                                                 |
| Integration             | Expo autolinking and CocoaPods through generated iOS project                                     |
| Release validation      | Simulator/device link, framework embed, ad-hoc signature verification, generic App Store archive |

Not supported: iOS before 15.1, `armv7`, Mac Catalyst, macOS, visionOS, tvOS,
watchOS, standalone CocoaPods, or SwiftPM. The generated Swift API is not a
stable SDK.

The current gates compile and archive a clean consumer. Device installation,
launch, signing with an adopter identity, App Store review, and real-service
wallet smoke testing remain M6 release-candidate gates.

The historical local M4 acceptance run used Xcode 26.6 (`17F113`). That is
evidence for the M4 candidate, not the M5 release baseline. Required M5 CI and
release evidence use Xcode 16.4 and CocoaPods 1.16.2.

## Android

| Property                      | Supported/tested value                                             |
| ----------------------------- | ------------------------------------------------------------------ |
| Minimum API                   | Android API `24`                                                   |
| Native ABIs                   | `arm64-v8a`, `x86_64`                                              |
| SDK command-line tools        | `14742923`                                                         |
| NDK                           | `27.1.12297006`                                                    |
| JDK for consumer release gate | JDK `17.0.19+10`                                                   |
| Native runtime dependency     | `net.java.dev.jna:jna:5.17.0@aar`                                  |
| JNA AAR SHA-256               | `4dbeffffa665d97ad5aa7eee297531d3c841a86716ab7f774fd6956422b3cf38` |
| Integration                   | Expo module Gradle plugin, generated Kotlin, packaged `jniLibs`    |
| Release validation            | Clean offline Gradle release APK and exact packaged ABI inspection |

Not supported: API 23 or earlier, `armeabi-v7a`, `x86`, unsupported desktop
JVMs, standalone Maven artifacts, or a stable generated Kotlin API.

The current gate assembles and inspects a clean release APK. Physical-device
installation, launch, OEM-specific behavior, Play Console review, and
real-service wallet smoke testing remain M6 release-candidate gates.

## Rust and native build inputs

| Input                        | Exact value                                                                     |
| ---------------------------- | ------------------------------------------------------------------------------- |
| Rust toolchain               | `1.97.1` with `rustfmt`, Clippy, LLVM tools, and declared Apple/Android targets |
| Rust coverage tool           | `cargo-llvm-cov 0.8.6`                                                          |
| Midnight Ledger release line | `8.1.0`                                                                         |
| Ledger Git revision          | `02716c2c95d50654aeb3cb63bfd8386046e4ca7d`                                      |
| Runtime crate                | `midnight-native-runtime`, `publish = false`                                    |
| UniFFI                       | `0.32.0`                                                                        |
| Apple deployment target      | `15.1`                                                                          |
| Android API/NDK              | API `24`, NDK `27.1.12297006`                                                   |

These pins are wire, proof, transaction, checkpoint, and binary compatibility
inputs. A Ledger revision change requires a separate reviewed compatibility
record and new release artifacts.

## Network identifiers

The TypeScript API accepts `preview`, `preprod`, and `mainnet`. This means the
runtime recognizes and validates those identifiers. It does not mean:

- the project operates a service for any network;
- every third-party indexer/proof/node deployment is compatible;
- a current live network has been exercised by required CI; or
- the alpha is approved for production or mainnet funds.

All URLs and credentials are adopter-supplied. M3 tests use deterministic local
mocks. Live preview smoke testing with disposable material is an explicit M6
gate and never runs for untrusted pull requests.

## Native artifact matrix

| Package path                                                       | Required content                                              |
| ------------------------------------------------------------------ | ------------------------------------------------------------- |
| `ios/build/MidnightNativeRuntime.xcframework`                      | iOS arm64 and simulator arm64/x86_64 dynamic framework slices |
| `android/src/main/jniLibs/arm64-v8a/libmidnight_native_runtime.so` | AArch64 Android runtime                                       |
| `android/src/main/jniLibs/x86_64/libmidnight_native_runtime.so`    | x86-64 Android runtime                                        |

No consumer Rust, Cargo, NDK, binary downloader, or npm `postinstall` is
permitted. See [native distribution](./NATIVE_DISTRIBUTION.md) for inspection
and size gates.

## Compatibility guarantees

This is a prerelease:

- pin the exact package version;
- generated native bindings may change without notice;
- TypeScript APIs, checkpoint format, error timing, and native packaging may
  change in another alpha;
- no downgrade or cross-alpha checkpoint migration is promised; and
- support is limited to the latest published alpha and its exact tested matrix.

Every release must replace this candidate record with its own exact evidence.
See the [alpha support and upgrade policy](./ALPHA_SUPPORT.md).
