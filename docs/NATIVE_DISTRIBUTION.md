# Native distribution

M4 packages release-mode Rust libraries inside the npm tarball so SDK consumers
do not need Cargo, `rustc`, or `rustup`. Native outputs are generated during the
release build and remain ignored by Git.

This document records the native package contract. The complete tagged-source
lineage, M5 evidence bundle, and independent verification procedure are in
[binary provenance](./BINARY_PROVENANCE.md). Native artifacts remain private
build outputs until the M6 destination repository resolves and records final
distribution terms.

## Pinned build contract

- Rust `1.97.1` is selected by the root `rust-toolchain.toml`.
- Apple supports iOS `15.1` or later with device `arm64` and simulator
  `arm64`/`x86_64`.
- Android uses NDK `27.1.12297006`, API level `24`, and only `arm64-v8a` and
  `x86_64`.
- Android resolves `net.java.dev.jna:jna:5.17.0@aar`; the release gate verifies
  SHA-256 `4dbeffffa665d97ad5aa7eee297531d3c841a86716ab7f774fd6956422b3cf38`.

The machine-readable contract is
[`scripts/native-build-config.json`](../scripts/native-build-config.json).

## Release artifacts

Run `npm run build:native` before packing a release. It creates:

- `packages/react-native/ios/build/MidnightMobileRuntime.xcframework`;
- `packages/react-native/android/src/main/jniLibs/arm64-v8a/` and `x86_64/`;
- `artifacts/apple/MidnightMobileRuntime.xcframework.zip`; and
- native build inspection reports under `artifacts/native/`.

`npm run check:native` creates the final npm tarball, a reproducible standalone
Android archive, and `artifacts/native/SHA256SUMS.json`. The checksum manifest
covers every shipped runtime binary, both standalone archives, and the final
tarball. All of these paths are ignored and must be attached by release
automation rather than committed.

The local-prover-enabled candidate is 28,758,008 bytes across the two Android
libraries. Its compressed Apple XCFramework is 12,257,053 bytes with SHA-256
`e95979f8f1eb03b2b28df4e80d0eec566a470565028189bf0c1afad2dd96e525`; the
single-pass uncompressed tree fingerprint is
`76a216762510fa745bf5f95200ab07c6db8567df48368e157e9680614ea1ba55`. These remain
below the unchanged 30 MiB Android payload and 15 MiB compressed Apple budgets.
The full reproducibility gate must still confirm the candidate fingerprint
before release.

## Consumer validation

The release gates:

1. build each native target twice and compare byte-for-byte output;
2. inspect architectures, platform metadata, minimum OS version, dynamic
   dependencies, install names, the exact eight-function UniFFI ABI, and the
   five reviewed local-prover C exports;
3. scan native binaries for excluded wallet capabilities;
4. assemble the real npm tarball twice and verify identical SHA-256 digests;
5. install that tarball into clean Expo consumers with Rust removed from `PATH`;
6. build an Android release APK, verify the module's generated Kotlin, and
   assert that every packaged native library uses only the two supported ABIs;
7. compile the installed Apple bridge for device and simulator, link and embed
   the XCFramework, create a generic iOS archive, and strictly verify ad-hoc
   code signatures.

The generated direct Swift and Kotlin binding surfaces and the local-prover C
ABI are implementation details. Only the public TypeScript entrypoints are
supported for SDK adopters.

## Consumer compatibility

The native payload supports only:

- iOS 15.1+ on arm64 devices and arm64/x86_64 simulators; and
- Android API 24+ on `arm64-v8a` and `x86_64`.

Expo Go, unsupported ABIs, standalone CocoaPods/Maven/SwiftPM distribution, and
direct generated native APIs are outside the alpha contract. The exact Expo,
React Native, Xcode, JDK, NDK, Rust, and Ledger versions are recorded in the
[compatibility matrix](./COMPATIBILITY.md).

## Release interpretation

The hashes above record the local-prover candidate and may change when reviewed
source, toolchain, or release metadata changes. A downloadable release is
authoritative only when its own `artifacts/release/SHA256SUMS.json`, SBOM,
license report, and provenance record cover the attached tarball and native
archives from the same tag. A checksum proves byte identity, not safety,
compatibility, or permission to distribute.
