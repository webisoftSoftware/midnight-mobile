# Midnight Mobile: Long-Term Project Plan

## 1. Purpose

This document turns the accepted SDK scope in [`PLAN.md`](../PLAN.md) into an
implementation program that can be executed issue by issue.

The first objective is a privately tested, then publicly released,
React Native-first alpha:

```text
@1am/midnight-mobile@0.1.0-alpha.1
```

The alpha contains wallet-core functionality only. It is a fresh, sanitized
copy derived from the assessed 1AM runtime. The working 1AM repository remains
unchanged and does not consume this SDK during the alpha program.

## 2. Fixed constraints

These constraints remain in force unless the accepted product plan is revised:

1. Do not change, refactor, commit to, or add dependencies from the working 1AM
   repository.
2. Do not import the 1AM Git history. Copy source through an audited allowlist
   into this repository.
3. Exclude social, FT/NFT, contract verifier artifacts, 1AM gateway
   authentication, private fast sync, viewing-key upload, and operated-service
   defaults.
4. Keep the Rust runtime I/O-free. Networking and persistence remain host
   responsibilities.
5. Make React Native/Expo the only supported consumer surface for v1.
6. Ship prebuilt Apple and Android libraries inside the npm package. Consumers
   must not need Rust, Cargo, an NDK toolchain, or a binary downloader.
7. Treat Swift and Kotlin bindings as internal bridge implementation details.
8. Keep the Rust crate private to the repository with `publish = false`.
9. Pin the alpha to the tested Midnight Ledger 8.1.0 revision.
10. Use only synthetic test material and caller-supplied network endpoints.

## 3. Program outcomes

The program is complete when all of the following are true:

- A clean Expo SDK 55 / React Native 0.83 application can install the npm
  tarball and build for iOS and Android without Rust installed.
- The SDK can open a wallet, perform standard sync, report balances, restore an
  opaque checkpoint, build wallet and dApp transactions, invoke an external
  proof service, generate DUST operations, and submit to a node.
- The public API contains only the 19 accepted wallet-core commands and returns
  typed results.
- Source, generated bindings, binaries, package contents, fixtures, logs, and
  repository history pass the security and forbidden-content gates.
- Release artifacts are reproducible from the tagged source and include
  checksums, an SBOM, provenance, and an explicit compatibility record.
- At least two independent external applications complete an integration
  before shared-core or stable direct-native work is approved.

## 4. Milestone map

| Milestone | Result | Blocking gate |
|---|---|---|
| M0 — Bootstrap | Private planning repository and controls | Scope and provenance reviewed |
| M1 — Allowlisted extraction | Compiling wallet runtime copied without history | Source and license inventory complete |
| M2 — Wallet-core sanitization | Private features physically absent | Forbidden-content and wallet tests pass |
| M3 — React Native API | Typed SDK, standard transport, and example | Example works against mocked services |
| M4 — Native distribution | Prebuilt iOS and Android payloads in npm | Clean consumer builds without Rust |
| M5 — Security and CI | Auditable pull-request and release pipelines | All required checks enforced |
| M6 — Alpha release | Private RC followed by public `0.1.0-alpha.1` | Release checklist and maintainer approval |
| M7 — External validation | Evidence from real third-party integrations | Demand gate measured and documented |
| M8 — Demand-led evolution | Shared core or native APIs only when justified | Separate design review for each expansion |

Milestones are sequential. Work inside a milestone may run in parallel, but no
later milestone may weaken an earlier security or isolation gate.

## 5. Detailed work breakdown

### M0 — Repository bootstrap and project controls

Status: in progress.

1. Confirm the repository is private and `main` is the default branch.
2. Retain the planning-only root commit as the history boundary.
3. Add the accepted scope, this delivery plan, and source provenance.
4. Add issue templates for bugs, security-safe feature requests, and external
   integration reports.
5. Add pull-request templates requiring:
   - scope and user impact;
   - tests performed;
   - security and secret-handling impact;
   - generated artifact changes;
   - confirmation that no 1AM-private feature was introduced.
6. Configure labels for milestones, workstreams, platform, security, blocked
   work, and external demand.
7. Protect `main` before implementation begins:
   - pull requests required;
   - required CI checks;
   - no force pushes or branch deletion;
   - one maintainer approval for code, native binaries, or release workflows.
8. Add `CODEOWNERS` for Rust, React Native, native packaging, security docs, and
   workflows when maintainers are assigned.
9. Decide licensing only after the ownership audit. Add `LICENSE-MIT`,
   `LICENSE-APACHE`, and `NOTICE` before source lands.

Exit criteria:

- Repository visibility is private.
- Scope, provenance, and roadmap are committed.
- No runtime source has been copied.
- Branch controls are ready before the source-import pull request.

### M1 — Allowlisted source extraction

Goal: create the smallest compiling source snapshot without importing private
history or unrelated application code.

1. Record the exact read-only source commit in `PROVENANCE.md`.
2. Create `docs/EXTRACTION_ALLOWLIST.md` listing every permitted source path and
   why it is required.
3. Create `docs/THIRD_PARTY_INVENTORY.md` listing:
   - direct Rust, npm, JNA, UniFFI, and Expo dependencies;
   - exact versions or revisions;
   - licenses and required notices;
   - source and binary redistribution obligations.
4. Copy into a temporary staging directory, never directly from a Git clone
   with its `.git` directory.
5. Copy only:
   - generic Rust wallet, state, transaction, codec, session, sync, checkpoint,
     and runtime modules;
   - UniFFI definitions and binding-generation configuration;
   - generic TypeScript boundary, runtime API, and runtime types;
   - Swift/Kotlin bridge code required by the Expo module;
   - native build scripts after removing app-specific paths;
   - wallet-core tests and synthetic fixtures.
6. Do not copy app UI, network constants, storage adapters, preferences,
   telemetry, gateway session code, private fast-sync code, asset proof
   material, or unrelated workspace packages.
7. Normalize the new workspace into:
   - `crates/runtime`;
   - `packages/react-native`;
   - `examples/expo`;
   - `docs`;
   - `scripts`.
8. Keep upstream dependency revisions unchanged during the first compile so
   extraction failures are not mixed with dependency upgrades.
9. Commit the extraction on a dedicated branch only after:
   - Rust host tests compile;
   - generated bindings reproduce;
   - the file allowlist matches the staged tree;
   - secret and license scans pass.

Exit criteria:

- The copied runtime compiles in this repository.
- Every copied file appears in the allowlist and provenance record.
- No app code or private Git history is present.
- The working 1AM repository has the same commit and clean status as before.

### M2 — Wallet-core sanitization and API hardening

Goal: make private capabilities impossible to invoke or recover from the public
source and binaries.

1. Delete social modules, commands, snapshots, state models, namespaces, tests,
   and Compact logic.
2. Delete FT/NFT deployment, minting, recovery, holdings, metadata, verifier
   embedding, proof-asset loading, and related generated data.
3. Delete gateway challenge signing, session authentication, endpoint defaults,
   proof-service credentials, IPFS effects, and private fast-sync paths.
4. Reduce endpoint roles to `indexer`, `proof`, and `node`.
5. Replace the copied Rust command object containing a string kind and optional
   fields with a serde-tagged wallet-core enum.
6. Retain exactly these command families:
   - signing and codecs;
   - standard shielded, unshielded, and DUST synchronization;
   - transfers, dApp intents, balancing, DUST generation, and submission.
7. Reject unknown, removed, or malformed commands with `INVALID_ARGUMENT`.
8. Add domain separation to `signData` using a versioned, length-prefixed
   transcript containing the caller's domain and data.
9. Preserve:
   - two sessions by default;
   - one active operation per session;
   - generation fencing;
   - cancellation;
   - contiguous stream offsets;
   - idempotent batch receipts;
   - pending-submission state including `statusUnknown`.
10. Expose checkpoints as opaque versioned bytes and remove legacy migration
    inputs from the public API.
11. Add an exhaustive TypeScript command-to-result map and native response
    decoders. Keep `unknown` inside decoder boundaries only.
12. Add a forbidden-content scanner for source, TypeScript declarations,
    generated Swift/Kotlin, Rust symbols, binary strings, and npm contents.

Exit criteria:

- All wallet-core tests pass.
- Removed commands fail before execution.
- Forbidden-content scans find no private feature implementation or credential.
- Public declarations contain no `unknown` command results.
- Checkpoint and signing threat-model tests pass.

### M3 — React Native package and standard host transport

Goal: make the wallet-core runtime usable from a normal Expo development build.

1. Create `@1am/midnight-mobile` with:
   - compiled JavaScript and TypeScript declarations;
   - explicit `exports`;
   - a restrictive npm `files` allowlist;
   - Expo autolinking metadata;
   - peer dependency ranges matching the tested Expo and React Native versions.
2. Export:
   - `createMidnightRuntimeApi`;
   - `MidnightRuntimeProvider`;
   - `useMidnightRuntime`;
   - `createStandardMidnightTransport`;
   - session, command, result, snapshot, checkpoint, transport, error, and host
     types.
3. Remove app imports by injecting:
   - optional redaction-safe logger;
   - optional checkpoint store;
   - endpoint configuration;
   - HTTP and WebSocket implementations;
   - cancellation and progress callbacks.
4. Implement the standard transport:
   - direct indexer HTTP/WebSocket requests;
   - direct proof and balance requests;
   - direct node submission;
   - endpoint-specific headers;
   - bounded effect-loop execution;
   - timeout and abort propagation;
   - deterministic rejection versus ambiguous `statusUnknown` handling.
5. Require all service URLs and credentials from the adopter. Do not provide
   1AM defaults.
6. Provide only an in-memory checkpoint adapter for examples and tests.
7. Build an Expo example that demonstrates:
   - runtime creation and disposal;
   - wallet session open and close;
   - normal sync and progress;
   - snapshot and balances;
   - checkpoint round-trip;
   - transaction/proof/submission flow;
   - cancellation and recoverable errors.
8. Use mock services by default. Live preview-network tests require explicit
   CI secrets and never run for untrusted pull requests.

Exit criteria:

- TypeScript type tests cover every command and result.
- The example completes the mocked wallet lifecycle on both platforms.
- Network failures map to documented error outcomes.
- No default endpoint, API key, wallet seed, or persistent storage is bundled.

### M4 — Prebuilt native distribution

Goal: ensure SDK consumers never compile Rust.

Apple tasks:

1. Build release dynamic libraries for iOS arm64 and simulator arm64/x86_64.
2. Set and validate the dynamic install name and minimum iOS version.
3. Generate and package a dynamic XCFramework.
4. Embed the XCFramework through the npm package's podspec.
5. Validate framework embedding, code signing, simulator linking, device
   linking, and App Store archive creation.
6. Keep the compressed XCFramework release archive at or below 15 MB.

Android tasks:

1. Pin the NDK and Rust Android targets used by CI.
2. Build release libraries for `arm64-v8a` and `x86_64`.
3. Package libraries under the Expo module's `jniLibs` structure.
4. Include generated Kotlin and the required JNA runtime dependency.
5. Remove all consumer `preBuild` tasks that invoke Cargo.
6. Keep the combined native payload at or below 30 MB.

Package tasks:

1. Assemble native outputs only in release CI; do not commit large binaries to
   normal source history.
2. Include binaries directly in the npm tarball—no `postinstall` download.
3. Produce a SHA-256 manifest for every binary and the final tarball.
4. Attach standalone archives for inspection while treating their direct
   Swift/Kotlin interfaces as unsupported.
5. Install the tarball into a clean consumer project with Rust removed from
   `PATH`, then build iOS and Android release variants.

Exit criteria:

- Clean consumer builds succeed without Rust.
- All required architectures are present and no unsupported architecture is
  accidentally shipped.
- Package-size budgets pass.
- Binary checksums match the release manifest.

### M5 — Security, documentation, and continuous integration

Goal: make every public change and release reviewable and reproducible.

Pull-request CI:

1. Rust format, clippy, host tests, and source policy.
2. TypeScript format, lint, type checking, Jest, and API declaration tests.
3. Generated-binding reproducibility.
4. Android native build and example release build.
5. iOS simulator framework and example build.
6. Secret, dependency, vulnerability, license, and forbidden-content scans.
7. npm pack-list verification and package-size reporting.

Release CI:

1. Rebuild all native artifacts from a clean tagged checkout.
2. Verify that package version, Git tag, generated bindings, and compatibility
   metadata agree.
3. Produce the npm tarball, native archives, SHA-256 manifest, SBOM, license
   report, and build provenance.
4. Re-run clean consumer builds from the packed tarball.
5. Require manual approval before npm publication or repository visibility
   changes.
6. Publish npm with trusted provenance and create matching GitHub release
   notes.

Required documentation:

1. Quick start and clean Expo development-build setup.
2. Architecture and resumable effect protocol.
3. Complete public API reference and error model.
4. Network/proof-server configuration without operated defaults.
5. Checkpoint storage contract and production encryption requirements.
6. Threat model for seeds crossing JavaScript, Swift/Kotlin, and Rust.
7. Ledger/network/platform compatibility matrix.
8. Binary provenance and reproducible-build instructions.
9. Security reporting policy, contribution guide, code of conduct, and
   unofficial-project disclaimer.
10. Alpha limitations, support expectations, and upgrade policy.

Exit criteria:

- Required CI is green and enforced.
- A reviewer can trace every release artifact to tagged source.
- Documentation is sufficient for an external developer with no 1AM context.
- The npm package and example agree with the documented public API.

### M6 — `0.1.0-alpha.1` release

1. Freeze the release candidate commit.
2. Run the complete release pipeline without publishing.
3. Review the source tree, npm pack list, binary string scan, SBOM, dependency
   licenses, artifact sizes, and test results.
4. Install the candidate tarball in a separate clean Expo application.
5. Run a controlled preview-network smoke test:
   - open and sync a disposable wallet;
   - export and restore its checkpoint;
   - build and prove a transaction;
   - submit or intentionally cancel it;
   - confirm accepted, rejected, and ambiguous submission handling.
6. Verify no production or maintainer wallet material entered logs or
   artifacts.
7. Tag `v0.1.0-alpha.1`.
8. Publish the npm package with provenance and the matching GitHub release.
9. Change repository visibility to public only after publication approval and
   a final secret scan.
10. Monitor install failures, native crashes, package size, documentation gaps,
    and security reports.

Rollback:

- If publication is unsafe, keep the repository private and do not publish.
- If npm publication succeeds with a severe defect, deprecate the affected
  version; do not silently replace an immutable package.
- Correct the issue in a new patch alpha with a complete release pipeline.

### M7 — External validation

Goal: gather evidence before expanding the architecture.

1. Create an integration-report template capturing:
   - app and team;
   - Expo/React Native and platform versions;
   - network and endpoint model;
   - features exercised;
   - setup time and blockers;
   - native build results;
   - API gaps and operational requirements.
2. Support integrations through public issues without collecting wallet seeds,
   checkpoints, credentials, or private logs.
3. Track:
   - successful independent applications;
   - clean-install success rate;
   - time to first synchronized wallet;
   - native crash and build-failure categories;
   - binary-size complaints;
   - requested platforms and missing wallet operations.
4. Publish alpha patch releases for defects without widening scope.
5. Keep Ledger revision upgrades separate from feature changes and publish
   compatibility notes for each upgrade.
6. Review demand after two independent integrations or a concrete external
   blocker requires architectural expansion.

Exit criteria:

- At least two independent applications complete an integration, or a
  documented external requirement justifies the next investment.
- Known defects, unsupported cases, and compatibility limits are documented.
- Maintainers can support releases without relying on private 1AM systems.

### M8 — Demand-led evolution

Each item below requires its own design proposal and approval. None is implied
by a successful alpha.

#### Shared core with the 1AM application

Consider only when duplicated fixes create measurable maintenance cost.

Required proposal:

- ownership and release model;
- dependency direction;
- private extension composition;
- migration plan with no feature regression;
- checkpoint and API compatibility;
- rollback path.

#### Stable Swift and Kotlin SDKs

Consider only when external native consumers exist.

Required work:

- intentionally designed native APIs rather than generated bridge exposure;
- semantic-versioning and binary-compatibility policy;
- standalone CocoaPods/SwiftPM and Maven distribution;
- native examples, documentation, and compatibility testing.

#### crates.io publication

Consider only when Rust consumers request the runtime.

Required work:

- replace or version all unpublished/git-only dependencies as necessary;
- complete crate metadata and feature design;
- run `cargo package` and publish dry-runs;
- define Rust API stability and minimum supported Rust version.

#### Social or asset packages

Do not add these back to wallet core.

Any proposal requires:

- separate package and capability boundary;
- Compact contract and verifier-artifact ownership review;
- threat model and independent audit;
- binary-size impact;
- explicit external use case.

## 6. Issue breakdown

Create implementation issues in this order:

1. `M0: configure branch protection and repository templates`
2. `M0: complete ownership and dual-license audit`
3. `M1: define the source extraction allowlist`
4. `M1: inventory dependencies and redistribution obligations`
5. `M1: import the compiling wallet runtime snapshot`
6. `M1: reproduce UniFFI bindings in the new workspace`
7. `M2: remove social and private identity capabilities`
8. `M2: remove FT/NFT and verifier capabilities`
9. `M2: remove gateway authentication, IPFS, and private fast sync`
10. `M2: introduce the typed Rust wallet-core command enum`
11. `M2: add typed TypeScript command result mapping`
12. `M2: harden domain-separated signing and opaque checkpoints`
13. `M2: add forbidden-content scans`
14. `M3: package the Expo module and public TypeScript API`
15. `M3: implement the standard indexer/proof/node transport`
16. `M3: add injected logging and checkpoint host interfaces`
17. `M3: build the mocked clean Expo example`
18. `M4: produce and validate the dynamic Apple XCFramework`
19. `M4: produce and validate Android native libraries`
20. `M4: assemble prebuilt binaries into the npm tarball`
21. `M5: add pull-request CI and security scans`
22. `M5: add release CI, SBOM, checksums, and provenance`
23. `M5: complete public API, architecture, and security documentation`
24. `M6: run the private alpha release candidate`
25. `M6: publish 0.1.0-alpha.1 and make the repository public`
26. `M7: onboard and document the first external integration`

An issue may be split into smaller pull requests, but its exit criteria must
remain intact.

## 7. Branching and release policy

- `main` is always releasable and protected.
- Use short-lived feature branches and pull requests.
- Keep extraction, sanitization, transport, native packaging, and CI changes in
  reviewable commits rather than one bulk import.
- Do not commit generated release binaries to long-lived source history.
- Use semantic versions beginning with `0.1.0-alpha.1`.
- Tag releases as `v<package-version>`.
- Never reuse or move a published tag.
- Keep Ledger upgrades, feature changes, and packaging changes separate when
  practical.
- Record compatibility changes in release notes and the support matrix.

## 8. Risk register

| Risk | Mitigation | Release blocker |
|---|---|---|
| Private feature or credential leaks into public history | Allowlist copy, fresh history, secret and forbidden scans | Yes |
| Extracted code cannot build independently | Compile before sanitization; preserve pinned revisions initially | Yes |
| Static Apple archive makes npm impractical | Dynamic XCFramework and enforced size budget | Yes |
| Consumers unexpectedly compile Rust | Package prebuilt binaries; clean build with Rust absent | Yes |
| Checkpoints expose wallet activity | Opaque format, no default persistence, production encryption contract | Yes |
| Seed copies survive across FFI boundaries | Threat model, best-effort wiping, caller ownership documentation | Yes |
| Transport failures misreport submission status | Explicit `statusUnknown` and end-to-end failure tests | Yes |
| Alpha diverges from the 1AM implementation | Manual provenance-tracked ports; revisit only after demand gate | No |
| Upstream Ledger change breaks wire compatibility | Exact pinning and per-release compatibility matrix | Yes |
| Generated bindings become accidental stable APIs | Mark internal and postpone direct-native distribution | No |

## 9. Definition of done for every milestone

A milestone is complete only when:

1. Its deliverables and exit criteria are satisfied.
2. Tests and security checks are automated where repeatable.
3. Documentation describes the resulting behavior and limitations.
4. No unrelated scope expansion was introduced.
5. Generated and packaged artifacts were inspected, not merely built.
6. The working 1AM repository remains unchanged.
7. Follow-up risks and deferred work are recorded as issues rather than hidden
   in implementation notes.

## 10. Current next action

Complete M0 repository controls and the dual-license ownership audit. The first
source-code task must then be `M1: define the source extraction allowlist`; no
runtime source should enter this repository before that allowlist is reviewed.
