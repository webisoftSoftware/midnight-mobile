# Native distribution

M4 packages release-mode Rust libraries inside the npm tarball so SDK consumers
do not need Cargo, `rustc`, or `rustup`. Native outputs are generated during the
release build and remain ignored by Git.

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

- `packages/react-native/ios/build/MidnightNativeRuntime.xcframework`;
- `packages/react-native/android/src/main/jniLibs/arm64-v8a/` and `x86_64/`;
- `artifacts/apple/MidnightNativeRuntime.xcframework.zip`; and
- native build inspection reports under `artifacts/native/`.

`npm run check:native` creates the final npm tarball, a reproducible standalone
Android archive, and `artifacts/native/SHA256SUMS.json`. The checksum manifest
covers every shipped runtime binary, both standalone archives, and the final
tarball. All of these paths are ignored and must be attached by release
automation rather than committed.

The validated M4 payload is 23,270,216 bytes across the two Android libraries.
The compressed Apple XCFramework is 9,279,899 bytes with SHA-256
`2432bf15a977571c6ecd98b500b6a60e927999507da00dcd3da0e4fb52df2621`; its
uncompressed tree fingerprint is
`3c1ea3a246c4db6edd96ab0e67518fe7acf616acf28c45f4e3798e283e7720f0`. These remain
below the 30 MiB Android payload and 15 MiB compressed Apple budgets.

## Consumer validation

The release gates:

1. build each native target twice and compare byte-for-byte output;
2. inspect architectures, platform metadata, minimum OS version, dynamic
   dependencies, install names, and the exact eight-function UniFFI ABI;
3. scan native binaries for excluded wallet capabilities;
4. assemble the real npm tarball twice and verify identical SHA-256 digests;
5. install that tarball into clean Expo consumers with Rust removed from `PATH`;
6. build an Android release APK, verify the module's generated Kotlin, and
   assert that every packaged native library uses only the two supported ABIs;
7. compile the installed Apple bridge for device and simulator, link and embed
   the XCFramework, create a generic iOS archive, and strictly verify ad-hoc
   code signatures.

The generated direct Swift and Kotlin binding surfaces are implementation
details. Only the public TypeScript API is supported for SDK adopters.
