# Seed lifecycle threat model

Opening a wallet requires three 32-byte secret inputs:

- `nightExternalKey`;
- `zswapSeed`; and
- `dustSeed`.

These values cross JavaScript, the Expo Swift/Kotlin bridge, generated UniFFI
code, and Rust. There is no claim that seeds remain exclusively in native or
hardware-backed memory.

## Security goals

The SDK aims to:

- reject wrong-length inputs before native use;
- minimize the lifetime of SDK-owned temporary copies;
- clear mutable SDK-owned copies on success and failure paths;
- retain the native session copy only while the wallet session is open;
- avoid seeds in errors, logs, checkpoints, examples, and test fixtures; and
- make caller ownership and residual risks explicit.

The SDK does not aim to defend a wallet from arbitrary code execution inside the
application process, a compromised operating system, invasive debugging, or a
malicious dependency that can read JavaScript memory before the call.

## Data flow and ownership

```text
Application secret source
  |
  | caller-owned Uint8Array (caller must wipe)
  v
TypeScript wrapper
  |
  | cloned Uint8Array (SDK wipes after native open settles)
  v
Expo marshalling and Swift Data / Kotlin ByteArray
  |
  | bridge-owned mutable buffers (best-effort wipe)
  v
Generated UniFFI lift
  |
  | Rust Vec inputs (zeroized on error or moved)
  v
Rust SessionSecrets (retained and zeroized on close/drop)
```

Additional transient copies can be created by JavaScript engines, React Native
marshalling, Swift `Data` copy-on-write behavior, Kotlin/JNA, UniFFI, allocator
growth, compiler optimization, operating-system paging, crash capture, and
debuggers. Clearing the visible mutable buffer cannot prove that every
historical copy was overwritten.

### JavaScript caller

The arrays passed to `openWalletSession` remain caller-owned. The TypeScript
wrapper clones them and never clears the originals. The application must clear
all three arrays in a `finally` block after the promise settles:

```ts
const secrets = await acquireDedicatedSecretBuffers();
try {
  await controller.openWalletSession(config, secrets);
} finally {
  secrets.nightExternalKey.fill(0);
  secrets.zswapSeed.fill(0);
  secrets.dustSeed.fill(0);
}
```

Use dedicated 32-byte arrays, not views into a larger recovery phrase, key
bundle, or long-lived shared buffer. Do not retain them in React state, Redux,
query caches, closures, component props, error objects, developer tools, or
hot-reload state.

The SDK cannot wipe the source from which the application derived the arrays.
The application must separately clear decoded mnemonics, KDF input/output,
temporary strings, native key-store responses, and worker messages.

### TypeScript wrapper

`createMidnightRuntimeApi` validates all three lengths, clones the arrays, and
fills those clones with zero in `finally` after the native asynchronous call
settles. A loader error, bridge rejection, or native error follows the same
cleanup path.

JavaScript `Uint8Array.fill(0)` is a best-effort overwrite. The JavaScript
engine may have copied, optimized, paged, or retained memory outside that
specific backing store.

### Swift bridge

The Expo function receives the keys as `Data`, creates mutable local values,
passes them to UniFFI, and calls `resetBytes` on those values in `defer`. The
checkpoint temporary is cleared in the same scope.

Swift `Data` has copy-on-write and framework marshalling semantics. Wiping the
mutable locals does not prove that Expo, Foundation, or an earlier backing
allocation has no copy. Direct generated Swift APIs are unsupported and bypass
the reviewed Expo call contract.

### Kotlin bridge

The Expo function receives keys as `ByteArray`, passes them to UniFFI, and fills
the key arrays and temporary decoded checkpoint with zero in `finally`.

JVM, JNA, Expo, and native marshalling may create copies outside the arrays
visible to the bridge. Garbage collection is not secure deletion. Direct
generated Kotlin APIs are unsupported and bypass the reviewed Expo call
contract.

### Rust runtime

Rust validates and derives wallet material from the FFI-owned vectors. On
failure, inputs are zeroized before returning. On success, the vectors move into
`SessionSecrets`; they remain in native memory because later wallet operations
require them.

`SessionSecrets` zeroizes all three values when the session closes and again on
drop. Temporary fixed-size seed arrays used during derivation are also zeroized.
This is best-effort process-memory cleanup, not a secure enclave or
locked-memory guarantee.

`pause()` cancels work and exports checkpoints but does not close native
sessions. Therefore it does not clear `SessionSecrets`. Applications whose
threat model requires secrets to leave process memory on background, device
lock, or account switch must explicitly close the session and require
re-authentication before reopening.

## Threats and mitigations

| Threat                                          | SDK behavior                                                            | Adopter requirement                                                                |
| ----------------------------------------------- | ----------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| Caller retains seed arrays                      | SDK clones; originals remain untouched                                  | Wipe originals and all derivation inputs in `finally`.                             |
| Logs or crash reports capture secrets           | SDK events omit values and bodies                                       | Redact application, network, native, analytics, and crash tooling.                 |
| Malicious JavaScript dependency reads memory    | Out of scope once code executes in-process                              | Minimize dependencies, audit supply chain, lock versions, and isolate acquisition. |
| Bridge/runtime leaves copies                    | Reviewed paths wipe visible mutable buffers                             | Accept residual managed-memory risk; prohibit direct bridge calls.                 |
| App backgrounds with wallet open                | Provider pauses but keeps session                                       | Close on background/lock when policy requires memory eviction.                     |
| Process crash before cleanup                    | Cleanup cannot run                                                      | Disable sensitive memory capture and protect device/crash artifacts.               |
| Swap, hibernation, or device extraction         | No locked-memory guarantee                                              | Rely on supported encrypted devices and platform hardening.                        |
| Debugger, instrumentation, rooted/jailbroken OS | Not prevented                                                           | Detect or disallow environments according to application policy.                   |
| Compromised endpoint                            | Endpoints do not receive seed inputs from SDK                           | Still validate service identity and avoid application-side seed leakage.           |
| Checkpoint theft                                | Checkpoint is privacy-sensitive but distinct from caller seed ownership | Use authenticated, device-protected encryption.                                    |

## Application integration requirements

### Acquisition

- Obtain secrets only after user authorization appropriate to wallet risk.
- Prefer non-exportable derivation or the shortest available export lifetime.
- Avoid string representations; JavaScript strings cannot be reliably wiped.
- Never place a mnemonic or master recovery secret in this API. Derive the three
  required 32-byte values in a reviewed component.
- Do not reuse synthetic example material.

### Open

- Allocate three fresh, dedicated arrays.
- Keep no extra application copies.
- Do not log lengths together with wallet identifiers if that can aid
  correlation.
- Await the call and wipe in `finally`, including cancellation and error paths.
- Treat an unexpected app termination during open as possible residual-memory
  exposure.

### Active session

- Limit open duration and use the two-session capacity only when necessary.
- Serialize commands per session.
- Close sessions on logout, wallet switch, security-policy timeout, and any
  platform lifecycle event that requires native memory eviction.
- Do not assume a checkpoint clears or replaces native session secrets.
- Keep debugging, memory inspection, and hot reload disabled for sensitive
  evaluation.

### Close

- Await `closeWalletSession`; it invalidates the handle and clears Rust-owned
  session secrets.
- Call `dispose` for all controller-owned sessions during teardown.
- Treat a crash, forced kill, or native fatal error as a path where orderly
  cleanup was not confirmed.
- Revoke or clear application key-store sessions and derived material
  separately.

## Error and telemetry policy

Stable `MidnightRuntimeError` codes contain no secret content. Do not wrap an
error by attaching inputs, serialized command objects, checkpoint bytes, network
bodies, or complete native traces.

The SDK logger shape is intentionally limited. An injected `fetch`, WebSocket,
checkpoint store, React error boundary, crash reporter, and platform logger are
outside that guarantee. Configure each one before live wallet evaluation.

## Residual limitations

The alpha does not provide:

- a hardware-wallet or secure-enclave signing protocol;
- locked or nonpageable memory;
- proof that managed-runtime copies were erased;
- an application seed vault or mnemonic lifecycle;
- Android/iOS compromise detection or remote attestation;
- automatic close on device lock;
- process crash-dump sanitization; or
- stable direct Swift/Kotlin secret APIs.

An adopter that cannot accept these limitations should not use this alpha for
real wallet material.
