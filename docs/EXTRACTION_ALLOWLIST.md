# Source Extraction Allowlist

## Authority and default deny

This allowlist applies only to committed content at:

- source repository: `https://github.com/webisoftSoftware/one-am-wallet`;
- source commit: `dc9c0dd34dba9e19304859106ca2531f48590599`;
- source component: `apps/mobile/modules/expo-midnight-native`.

The commit is authoritative. Every source path not listed below is denied. No
glob, directory entry, moving branch, working-tree file, generated binary, or
Git history is authorized. This document does not authorize source to land until
the ownership and source-license gaps in
[`THIRD_PARTY_INVENTORY.md`](./THIRD_PARTY_INVENTORY.md) are resolved.

Extraction must use `git show` or `git archive` under section 2.1 of
[`LONG_TERM_PROJECT_PLAN.md`](./LONG_TERM_PROJECT_PLAN.md). Content first enters
a temporary directory outside the source repository. The source checkout's
complete HEAD and porcelain output must remain byte-identical.

Modes in the tables below are:

- **Copy** — the file contains a reusable wallet-core basis. Normal path,
  package-name, formatting, and quality-policy changes are still required.
- **Select** — the file is co-mingled with excluded or application-specific
  behavior. It may enter temporary staging for surgical sanitization, but the
  upstream file itself must never land. Only the stated wallet-core behavior may
  survive in a newly reviewed target file.

The deny rules and required removals in this document override every permitted
path.

## Audit bracket status

This is a candidate allowlist derived from immutable Git objects at the assessed
commit. The 2026-07-29 read-only audit did not satisfy the required source-state
bracket: the source checkout's HEAD remained unchanged, but its complete
porcelain output changed during the audit. No source was copied.

Before this allowlist can authorize staging or landing, repeat the entire audit
from a fresh HEAD and full-porcelain capture and require byte-identical
post-audit output. The ownership and source-license blocker must also be
resolved.

## Package and native build basis

| Mode   | Exact source path                                                               | Permitted reason and required treatment                                                                 |
| ------ | ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| Select | `apps/mobile/modules/expo-midnight-native/package.json`                         | Package metadata basis only; replace the 1AM identity and audited-source license claim.                 |
| Copy   | `apps/mobile/modules/expo-midnight-native/expo-module.config.json`              | Expo autolinking module registration for Apple and Android.                                             |
| Select | `apps/mobile/modules/expo-midnight-native/android/build.gradle`                 | Android library/JNA configuration only; delete consumer-side Rust compilation and app-project wiring.   |
| Select | `apps/mobile/modules/expo-midnight-native/ios/ExpoMidnightNative.podspec`       | Pod/autolinking layout only; replace 1AM identity and consume only packaged prebuilt artifacts.         |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/Cargo.lock`                      | Preserve the assessed first-compile dependency resolution.                                              |
| Select | `apps/mobile/modules/expo-midnight-native/rust/Cargo.toml`                      | Rust workspace/profile basis; normalize paths and set the reviewed project license.                     |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/bindgen/Cargo.toml`              | Private UniFFI binding-generator package definition.                                                    |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/bindgen/src/main.rs`             | UniFFI binding-generator entry point.                                                                   |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/clippy.toml`                     | Assessed Rust lint configuration.                                                                       |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/runtime/Cargo.toml`              | Private runtime crate and pinned Ledger dependency definitions.                                         |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/uniffi.toml`                     | UniFFI binding configuration.                                                                           |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/rustfmt.toml`                    | Assessed Rust formatting configuration.                                                                 |
| Select | `apps/mobile/modules/expo-midnight-native/scripts/build-android.sh`             | CI-time Android native build basis; remove application-relative assumptions and consumer invocation.    |
| Select | `apps/mobile/modules/expo-midnight-native/scripts/build-ios.sh`                 | CI-time Apple XCFramework build basis; remove application-relative assumptions and consumer invocation. |
| Select | `apps/mobile/modules/expo-midnight-native/scripts/check-rust-source-policy.mjs` | Rust source-policy basis; replace upstream roots and add the wallet-core forbidden-content policy.      |
| Select | `apps/mobile/modules/expo-midnight-native/scripts/check-rust.sh`                | Rust quality wrapper basis; normalize repository paths and pinned commands.                             |
| Select | `apps/mobile/modules/expo-midnight-native/scripts/generate-bindings.sh`         | Binding-generation basis; normalize paths and generate only from sanitized Rust.                        |
| Select | `apps/mobile/modules/expo-midnight-native/scripts/test-host.sh`                 | Host-test wrapper basis; normalize repository paths and fixture handling.                               |

Generated Swift/Kotlin bindings and native libraries are not source inputs. They
must be regenerated from the sanitized runtime and verified reproducible.

## TypeScript and native bridge basis

| Mode   | Exact source path                                                                                                        | Permitted reason and required treatment                                                                                                                         |
| ------ | ------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Select | `apps/mobile/modules/expo-midnight-native/index.ts`                                                                      | Export/decoder basis; remove the application logger import and expose only the accepted SDK surface.                                                            |
| Select | `apps/mobile/modules/expo-midnight-native/native-boundary.ts`                                                            | Expo/native loading, base64, and native-error normalization basis; repair strict indexing and keep `unknown` only at decoder boundaries.                        |
| Select | `apps/mobile/modules/expo-midnight-native/runtime-api.ts`                                                                | Runtime adapter/decoder basis; remove gateway signing, `gateway`/`ipfs` roles, legacy checkpoint exposure, and removed command results.                         |
| Select | `apps/mobile/modules/expo-midnight-native/runtime-command-types.ts`                                                      | Wallet command type basis; retain exactly the 19 commands accepted in `PLAN.md` and the three endpoint roles.                                                   |
| Select | `apps/mobile/modules/expo-midnight-native/runtime-types.ts`                                                              | Session/effect/snapshot type basis; remove social, FT/NFT, gateway, legacy/plaintext checkpoint, fast-sync, and public `unknown` result types.                  |
| Select | `apps/mobile/modules/expo-midnight-native/MidnightRuntimeProvider.tsx`                                                   | React provider lifecycle basis; replace app storage/preferences/logging with injected `CheckpointStore`, `Logger`, transport, and endpoint configuration.       |
| Select | `apps/mobile/modules/expo-midnight-native/android/src/main/java/expo/modules/midnightnative/ExpoMidnightNativeModule.kt` | Handwritten Expo Kotlin bridge basis; remove deleted exports and consume regenerated wallet-core bindings only.                                                 |
| Select | `apps/mobile/modules/expo-midnight-native/ios/ExpoMidnightNativeModule.swift`                                            | Handwritten Expo Swift bridge basis; remove deleted exports and consume regenerated wallet-core bindings only.                                                  |
| Select | `apps/mobile/modules/expo-midnight-native/__tests__/adapter.test.ts`                                                     | Strict boundary/error tests; remove upstream package and logger assumptions.                                                                                    |
| Select | `apps/mobile/modules/expo-midnight-native/__tests__/runtime-adapter.test.ts`                                             | Decoder and zeroization regression basis; delete gateway, FT/NFT, social, fast-sync, and legacy-checkpoint cases and add the exhaustive wallet-core result map. |
| Select | `apps/mobile/modules/expo-midnight-native/__tests__/runtime-provider-fixtures.ts`                                        | Provider mock structure only; replace gateway and legacy-state members with deterministic wallet-core-only values.                                              |
| Select | `apps/mobile/modules/expo-midnight-native/__tests__/runtime-provider.test.ts`                                            | Provider lifecycle/cancellation basis; replace application persistence and collapsed/fast-sync behavior with injected host interfaces and standard sync.        |

## Rust wallet-core basis

| Mode   | Exact source path                                                                                      | Permitted reason and required treatment                                                                                                          |
| ------ | ------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/codec.rs`                                   | Generic Ledger codec and proving-payload helpers; verifier-key inputs are caller-supplied data, not embedded verifier artifacts.                 |
| Select | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/lib.rs`                                     | UniFFI facade and error basis; delete all asset/social modules and gateway exports.                                                              |
| Select | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime.rs`                                 | Session/runtime coordinator basis; delete excluded module includes, operation variants, and tests.                                               |
| Select | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/checkpoint.rs`                      | Checkpoint integrity/journal basis; keep the representation private and expose only opaque versioned bytes.                                      |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/codec.rs`                           | Runtime codec command helpers.                                                                                                                   |
| Select | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/commands/codecs.rs`                 | Accepted signing/codec commands only; replace `signData` with the required versioned, length-prefixed domain transcript.                         |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/commands/dapp.rs`                   | Accepted dApp transfer and intent construction.                                                                                                  |
| Select | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/commands/deploy_and_balance.rs`     | Retain only `balanceUnsealed` and `balanceSealed`; all FT/NFT deployment code is denied.                                                         |
| Select | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/commands/session_and_sync.rs`       | Retain accepted signing and standard-sync commands; delete gateway signing, viewing-key upload, `/chain-data/v2`, and private fast sync.         |
| Select | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/commands/submission_and_queries.rs` | Retain only `submitFinalized`; all contract, FT/NFT, and social queries are denied.                                                              |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/commands/transfers.rs`              | Accepted transfer and DUST generation command handling.                                                                                          |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/operation_helpers.rs`               | Generic operation, argument, and caller-supplied proving-material helpers.                                                                       |
| Select | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/registry.rs`                        | Operation registry/concurrency basis; retain wallet-core operations only.                                                                        |
| Select | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/registry_types.rs`                  | Operation state basis; delete social, contract-query, mint, launch, IPFS, and asset-proof variants.                                              |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/resume/balance.rs`                  | Accepted external balance/proof continuation.                                                                                                    |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/resume/dapp.rs`                     | Accepted dApp proof continuation.                                                                                                                |
| Select | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/resume/finalize.rs`                 | Generic finalization/submission continuation only; delete excluded operation arms.                                                               |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/resume/generate_dust.rs`            | Accepted DUST proof continuation.                                                                                                                |
| Select | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/session.rs`                         | Session state/fencing basis; delete gateway challenge signing and keep lifecycle I/O host-injected.                                              |
| Select | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/types.rs`                           | Runtime serialization basis; replace the string-plus-optional command object with the accepted serde-tagged enum and delete private fields.      |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/transaction.rs`                             | Generic transaction validation, balancing, and finalization.                                                                                     |
| Select | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/transaction/remote_proof.rs`                | Retain caller-supplied proving-material coordination only; remove remote key material used solely by excluded asset flows.                       |
| Select | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/wallet_state.rs`                            | Wallet-state module basis; delete private fast-sync constants/imports and retain standard host-fed state transitions.                            |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/wallet_state/apply_and_prepare.rs`          | Generic wallet-state transaction application and preparation.                                                                                    |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/wallet_state/dapp.rs`                       | Generic dApp wallet-state construction.                                                                                                          |
| Select | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/wallet_state/restore.rs`                    | Generic state restoration only; delete private fast-sync container decoders and accept only the reviewed internal checkpoint representation.     |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/wallet_state/shielded_and_state.rs`         | Generic shielded state and snapshot operations.                                                                                                  |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/wallet_state/spend_builders.rs`             | Generic shielded, unshielded, and DUST transaction builders.                                                                                     |
| Select | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/wallet_state/sync.rs`                       | Retain host-fed standard sync transitions only; delete viewing-key upload, private routes, collapsed/v2 private containers, and fast-sync logic. |
| Select | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/wallet_state/types.rs`                      | Generic wallet wire/snapshot types only; delete private fast-sync container types and limits.                                                    |

## Wallet-core test basis

| Mode   | Exact source path                                                                                   | Permitted reason and required treatment                                                                                         |
| ------ | --------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/tests/codecs_and_submission.rs`  | Accepted codec, submission, and malformed-input behavior.                                                                       |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/tests/properties.rs`             | Deterministic runtime property tests.                                                                                           |
| Select | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/tests/session_and_operations.rs` | Retain session fencing/cancellation tests only; delete gateway, FT/NFT/query cases and replace upstream fixture secrets.        |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/tests/stress.rs`                 | Session/operation concurrency and stress behavior.                                                                              |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/transaction/tests.rs`                    | Generic transaction codec, validation, and proof-continuation tests.                                                            |
| Select | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/wallet_state/dapp_tests.rs`              | Generic dApp state tests after replacing upstream fixture secrets with new documented synthetic material.                       |
| Select | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/wallet_state/tests/snapshots.rs`         | Generic snapshot/checkpoint tests after removing legacy public inputs and replacing upstream fixture secrets.                   |
| Select | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/wallet_state/tests/sync.rs`              | Retain standard contiguous-offset/idempotency/malformed-input cases; delete fast-sync, viewing-key, `/chain-data/v2`, and IPFS. |
| Copy   | `apps/mobile/modules/expo-midnight-native/rust/runtime/src/wallet_state/tests/transfers.rs`         | Generic shielded/unshielded transfer and DUST transaction tests.                                                                |

No upstream fixture file is permitted. New fixtures must be generated in this
repository from documented deterministic synthetic inputs. In particular, the
assessed `ledger-8.1.0.json` labels fixed secret material `productionSecrets`,
so it cannot satisfy the synthetic-fixture gate without independent evidence and
renaming.

## Explicit exclusions

The following exact paths are denied even for temporary selective sanitization:

- `apps/mobile/modules/expo-midnight-native/rust/runtime/resources/manifest.json`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/resources/v1/ft-launch.verifier.gz`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/resources/v1/ft-unshielded-launch.verifier.gz`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/resources/v1/personal-mint.verifier.gz`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/resources/v1/personal-unshielded-mint.verifier.gz`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/asset_mint.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/asset_nft.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/asset_verifiers.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/assets.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/assets/call.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/assets/deploy.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/assets/tests.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/contracts.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/contracts/nft_holdings.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/contracts/queries.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/contracts/tests.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/social.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/social/call.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/social/decode.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/social/publish_post.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/social/set_account_mode.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/social/set_reaction.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/social/state.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/social/tests.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/social/transcript.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/social/update_relationship.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/social/upsert_profile.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/commands/assets_and_social.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/resume/asset_proof.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/resume/assets.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/resume/contracts.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/resume/social_load.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/resume/social_write.rs`
- `apps/mobile/modules/expo-midnight-native/rust/runtime/src/runtime/tests/assets_social_and_transfer.rs`
- `apps/mobile/modules/expo-midnight-native/ios/generated/MidnightNativeRuntime.swift`
- `apps/mobile/modules/expo-midnight-native/fixtures/codec-8.1.0.json`
- `apps/mobile/modules/expo-midnight-native/fixtures/ledger-8.1.0.json`
- `apps/mobile/modules/expo-midnight-native/fixtures/transaction-8.1.0.json`
- `apps/mobile/modules/expo-midnight-native/fixtures/wallet-state-8.1.0.json`

The component `.gitignore`, README, application UI, storage adapters,
preferences, logger, endpoint constants, telemetry, and every path outside the
component are also denied.

## Landing gates

Before any sanitized source moves from temporary staging into this repository:

1. Resolve and record the source ownership/license blocker in
   [`THIRD_PARTY_INVENTORY.md`](./THIRD_PARTY_INVENTORY.md).
2. Confirm the staged input path set is an exact subset of this allowlist.
3. Confirm every **Select** target contains only the stated wallet-core
   behavior.
4. Scan source and generated text for social, FT/NFT, verifier artifacts,
   gateway signing, `/chain-data/v2`, viewing-key upload, IPFS, private fast
   sync, operated endpoints, credentials, and upstream application paths.
5. Generate bindings only from the sanitized Rust runtime.
6. Generate new deterministic synthetic fixtures and record their recipes.
7. Re-run the source checkout HEAD and full porcelain checks and require
   byte-identical output.
