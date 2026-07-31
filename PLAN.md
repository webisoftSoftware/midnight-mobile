# Fresh 1AM Midnight Mobile SDK Alpha

This document records the accepted product and technical scope. The detailed
delivery sequence, work breakdown, gates, and long-term roadmap are maintained
in [docs/LONG_TERM_PROJECT_PLAN.md](./docs/LONG_TERM_PROJECT_PLAN.md).

## Summary

Create this repository as a fresh, sanitized project without modifying or
creating dependencies from the working 1AM repository.

Release `@1am/midnight-mobile@0.1.0-alpha.1` as a React Native/Expo-first
wallet-core SDK. Social, FT/NFT, verifier artifacts, gateway authentication,
private fast sync, and all other 1AM application logic are excluded from the
source, package, binaries, tests, and Git history.

This alpha intentionally duplicates the sanitized wallet runtime. Shared-core
migration, extensions, crates.io, checkpoint migration, and stable direct-native
APIs are deferred until external demand is demonstrated.

Any later 1AM consumer migration is an ordered M9 program, not part of the
alpha. Before changing 1AM consumer code, the SDK must rename every colliding
native identifier, retain the permanent 81.31% Rust coverage gate established by
M5, execute the real native runtime on a device, resolve signature
compatibility, prove disposable non-production 1AM checkpoint interop, and pass
a Ledger 8.1.0 golden-vector/transcript equivalence harness. M9 may add only two
normal-command-path mint migration hooks: session-seed shielded mint context and
`watch_shielded_mint` registration for an externally built coin. These are not
FT/NFT product workflows and do not authorize verifier artifacts.

## Repository and sanitization

- Populate the repository through an explicit file allowlist, not by cloning,
  filtering, or rewriting the 1AM repository.
- Use one audited initial source commit containing no private Git history.
- Record the source commit and extraction date in `PROVENANCE.md`; fixes are
  manually ported during the alpha period.
- Leave the working 1AM repository byte-for-byte unchanged: no refactor,
  submodule, package dependency, shared crate, checkpoint migration, or build
  change.
- Use this compact layout:
  - `crates/runtime` — wallet-core Rust runtime and UniFFI facade.
  - `packages/react-native` — the single npm/Expo package.
  - `examples/expo` — clean integration and release smoke test.
  - `docs` — architecture, API, security, compatibility, and release notes.
  - `.github` and `scripts` — CI, binary builds, audits, and packaging.

Remove from the copy:

- All social commands, types, state, tests, namespaces, and Compact logic.
- FT/NFT deployment, minting, recovery, metadata, and holdings queries.
- Embedded verifier artifacts and related build steps.
- `signGatewayChallenge`, gateway sessions, gateway credentials, and 1AM
  endpoint defaults.
- `/chain-data/v2`, viewing-key upload, authenticated fast sync, and other
  private gateway routes.
- IPFS effects and app-only storage, preferences, logging, UI, and network
  constants.

## Public SDK surface

Expose wallet session management, standard synchronization, snapshots and
balances, opaque checkpoint export/restore, transfers, DUST generation, dApp
transactions, transaction codecs, proving/balancing effects, cancellation, and
submission tracking.

The public command union contains only these 19 commands:

- Signing/codecs: `signData`, `createCheckPayload`, `parseCheckResult`,
  `createProvingPayload`, `canonicalizeTransaction`.
- Sync: `createSyncRequest`, `createShieldedSpentRequest`,
  `applyShieldedSpentResponse`, `setShieldedProtocolVersion`,
  `createDustSpendRequest`, `createDustCommitmentRequest`,
  `applyDustSpendResolution`.
- Transactions: `transfer`, `dappTransfer`, `dappIntent`, `generateDust`,
  `balanceUnsealed`, `balanceSealed`, `submitFinalized`.

Implementation requirements:

- Replace the copied string-plus-optional-fields Rust command structure with a
  wallet-core-only serde-tagged enum. Unknown or removed command kinds fail as
  `INVALID_ARGUMENT`.
- Add an exhaustive TypeScript command-to-result map; no public command returns
  `unknown`.
- Preserve the resumable effect protocol, generation fencing, cancellation,
  contiguous sync validation, duplicate-batch handling, and `statusUnknown`
  submission outcome.
- Restrict endpoint roles to `indexer`, `proof`, and `node`.
- Require a domain in `signData` and sign a versioned, length-prefixed
  domain/data transcript.
- Default to two concurrent sessions and one active operation per session.
- Export `createMidnightRuntimeApi`, `MidnightRuntimeProvider`,
  `useMidnightRuntime`, `createStandardMidnightTransport`, `CheckpointStore`,
  `Logger`, network configuration, commands, results, snapshots, and errors.
- Support `preview`, `preprod`, and `mainnet` identifiers, but require adopters
  to supply indexer, proof-server, and node URLs. Include no operated-service
  defaults or credentials.

The standard transport performs direct HTTP/WebSocket requests for normal
indexer sync, proof/balance effects, and node submission. It accepts injected
fetch, WebSocket, endpoint-header, timeout, and cancellation implementations.

## Checkpoints and security

- Present checkpoints to JavaScript as opaque versioned bytes; keep the native
  representation private.
- Provide optional `CheckpointStore` injection and an in-memory test adapter. Do
  not ship AsyncStorage persistence or automatic migration.
- Document that checkpoints contain privacy-sensitive wallet state and that the
  internal checksum detects corruption, not tampering or disclosure.
- Require production adopters to implement authenticated, device-protected
  encryption.
- Document accurately that seeds cross JavaScript, Swift/Kotlin, and Rust
  boundaries. Wipe SDK-owned copies on all paths and require callers to wipe
  their original buffers.
- Use only deterministic synthetic fixtures. Remove or rename misleading
  `productionSecrets` and secret-key fixtures.
- Scan allowlisted source, generated bindings, native binaries, the npm tarball,
  and the initial source commit for secrets and forbidden content.
- If a potentially live credential is discovered, exclude it and rotate it
  externally without changing the 1AM repository.
- Keep repository-authored bootstrap material under `MIT OR Apache-2.0` and
  preserve the assessed component's MIT declarations and provenance during
  internal implementation. Defer the extracted implementation's final copyright,
  attribution, and distribution terms to the destination-repository migration.
  Resolve them before any npm publication or public distribution, then include
  the chosen license texts, dependency attribution, `NOTICE`, `SECURITY.md`, and
  an explicit unofficial/community-maintained disclaimer.
- Keep the Rust crate `publish = false` and pinned to the currently tested
  Midnight Ledger `8.1.0` revision with a committed lockfile.

## React Native packaging

Publish one npm artifact: `@1am/midnight-mobile@0.1.0-alpha.1`.

- Target Expo SDK 55 / React Native 0.83 initially.
- Build a dynamic Apple XCFramework for arm64 devices and arm64/x86_64
  simulators.
- Build Android libraries for `arm64-v8a` and `x86_64`.
- Assemble generated Swift/Kotlin bindings and prebuilt libraries into the npm
  tarball using its `files` allowlist.
- Use the npm package's podspec and Gradle module for React Native autolinking.
  Do not publish standalone CocoaPods, Maven, or SwiftPM products.
- Remove consumer-side Rust compilation from CocoaPods and Gradle tasks.
- Do not use an npm `postinstall` downloader; all required binaries are present
  in the tarball.
- Attach native archives, a SHA-256 manifest, SBOM, provenance, and npm tarball
  to the matching GitHub release.
- Label generated Swift/Kotlin bindings as internal React Native implementation
  details, not stable public APIs.

## CI and release gates

For every pull request:

- Run the pinned repository quality command covering Markdown, TypeScript, Rust,
  shell scripts, GitHub Actions, formatting, type checking, and tests.
- Enforce a 500-physical-line maximum for handwritten source files, or 800 for
  Rust, with reviewed exceptions only for generated bindings, lockfiles,
  vendored code, and machine-generated fixtures.
- Reject unqualified lint suppressions, TypeScript `any`, unsafe Rust without a
  documented safety invariant, and warnings from supported toolchains.
- Run strict Rust clippy, strict TypeScript/ESLint rules, wallet-core tests,
  coverage gates, and API type tests.
- Run dependency, license, secret, and forbidden-symbol scans.
- Build Android native libraries and an Android release example.
- Build the iOS simulator framework and example on macOS.
- Verify generated bindings and committed API declarations are reproducible.

For the `v0.1.0-alpha.1` release:

- Build all native targets from a clean checkout.
- Validate Apple architectures, dynamic install names, embedding, code signing,
  CocoaPods integration, and an App Store archive.
- Validate Android ABIs and release packaging.
- Run `npm pack`, inspect its exact contents, and reject unexpected files,
  source maps containing private paths, secrets, or removed features.
- Install the packed tarball into a clean Expo example with Rust absent from
  `PATH`; build and launch both platforms.
- Exercise wallet open, normal sync, snapshots and balances, checkpoint
  round-trip, shielded and unshielded transfers, DUST, dApp flows, proof
  effects, cancellation, and accepted/rejected/unknown submissions on controlled
  testnet fixtures.
- Require a negative scan showing no social or asset commands, verifier blobs,
  `1am:` social namespaces, gateway signing, private endpoints, credentials, or
  fast-sync routes in source, declarations, generated bindings, and binaries.
- Keep the compressed Apple artifact at or below 15 MB and the combined Android
  native payload at or below 30 MB.
- Publish npm with provenance through a manually approved GitHub release
  environment, then make the repository public.

## Deferred work and alpha exit

Explicitly postpone:

- Refactoring the working 1AM repository or making it consume the SDK.
- A shared Rust/TypeScript core or private extension framework.
- Social or FT/NFT packages.
- Legacy/plaintext checkpoint migration.
- crates.io publication.
- Standalone CocoaPods, Maven, or SwiftPM distribution.
- Stable Swift and Kotlin APIs.

Collect demand through GitHub issues and the example integration. Reconsider
shared-core migration only after at least two independent external applications
complete an integration or a concrete outside contributor requires native or
extension support.

After that demand/design review, M9 consumer migration prerequisites remain
blocking and ordered. Social stays a stateless internal 1AM module because its
six commands need only the already-public `networkId` and
`shieldedCoinPublicKeyHex`; it does not expand this SDK.
