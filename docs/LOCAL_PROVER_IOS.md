# iOS local prover

The experimental iOS local prover implements the Ledger 8.1.0 proof server's
binary `/check` and `/prove` behavior inside the SDK process. Applications use
the same separate `@1am/midnight-mobile/local-prover` package subpath documented
in the [Android guide](LOCAL_PROVER_ANDROID.md); the root TypeScript API and the
runtime's eight-function UniFFI ABI remain unchanged.

## Artifact locations

iOS accepts two file URI forms:

- `bundle://path/to/file` resolves below the application bundle's resource
  directory; and
- an absolute path resolves below the application's sandbox home directory.

Relative paths, bundle or sandbox escapes, control characters, symbolic-link
escapes, empty files, files larger than 512 MiB, configurations larger than 2
GiB, size mismatches, and non-lowercase SHA-256 values fail configuration.
Duplicate parameter `k` values and duplicate circuit key locations are also
rejected before any file is mapped.

Package artifacts uncompressed in the application bundle or provision them in
the application sandbox. The Swift bridge opens them read-only, maps them with
POSIX `mmap`, and retains the mappings until `close()`. Rust verifies every
declared digest and eagerly decodes the parameters, IR, prover key, and verifier
key before publishing the new registry. The SDK never downloads or writes proof
artifacts.

## Execution and lifecycle

Expo schedules configuration, checking, proving, and close away from the main
thread. Bridge state is held only long enough to take an immutable snapshot;
Rust owns process-wide serialization and returns `PROVER_BUSY` to a concurrent
caller. Rust uses a fixed two-thread Rayon pool. Upstream proving is not
cooperatively cancellable, so this API exposes no cancellation contract.

Request bytes are copied into owned mutable storage for the synchronous native
call and best-effort wiped afterward. Every native response is copied into
Swift-owned `Data` and freed through the matching Rust allocator on success and
failure paths. Call `close()` only after outstanding operations settle.

The stable error codes and transport routing rules are identical to Android.
Only proof-role `check` and `prove` effects are intercepted; fee balancing,
submission, indexer, and other effects remain remote.

## Validation boundary

[`tools/ios-local-prover`](../tools/ios-local-prover/README.md) builds a Release
Expo consumer that imports only the local-prover package subpath, maps the
pinned k=15 Zswap artifacts from its bundle, completes `/check` and `/prove`,
and validates both returned values with the host Rust codec. The installed arm64
Simulator run proves Apple packaging and Swift/C/Rust integration. A Simulator
uses host CPU and memory, so it does not establish physical-iPhone performance
or memory feasibility.

The focused Release run on 2026-07-31 used an arm64 iPhone 17 Simulator on iOS
26.5. Configuration took 1,038 ms, `/check` took 4 ms, and `/prove` took 1,736
ms. The host validator accepted the 47-byte check response and 4,860-byte tagged
V2 proof. The app bundle was 55,061,057 bytes including 17,315,450 bytes of
proving artifacts; the embedded native framework was 13,124,220 bytes, and the
SDK XCFramework archive was 12,257,053 bytes. Reproduce it with:

```sh
node tools/ios-local-prover/scripts/run-simulator.mjs
```
