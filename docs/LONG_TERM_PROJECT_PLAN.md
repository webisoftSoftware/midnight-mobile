# Midnight Mobile: Long-Term Project Plan

## 1. Purpose

This document turns the accepted SDK scope in [`PLAN.md`](../PLAN.md) into an
implementation program that can be executed issue by issue.

The first objective is a privately tested, then publicly released, React
Native-first alpha:

```text
@1am/midnight-mobile@0.1.0-alpha.1
```

The alpha contains wallet-core functionality only. It is a fresh, sanitized copy
derived from the assessed 1AM runtime. The working 1AM repository remains
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

### 2.1 Read-only source reference

Use the following repository only as a read-only source reference:

- Repository:
  [`webisoftSoftware/one-am-wallet`](https://github.com/webisoftSoftware/one-am-wallet)
- Assessed commit: `dc9c0dd34dba9e19304859106ca2531f48590599`
- Assessed component: `apps/mobile/modules/expo-midnight-native`
- Provenance record: [`PROVENANCE.md`](../PROVENANCE.md)

The commit hash is authoritative. Do not read from a moving branch, pull a newer
revision, or silently substitute the current working tree. Updating the source
revision requires a reviewed change to `PROVENANCE.md`, the extraction
allowlist, dependency inventory, and compatibility record.

Permitted source-repository operations are limited to:

- `git rev-parse`, `git status`, `git show`, `git diff`, and `git archive`;
- read-only file listing and searching;
- reading files and metadata from the pinned commit;
- writing archive output into a separate temporary staging directory.

Do not run any command that changes the source checkout, index, refs, object
database, ignored files, untracked files, dependency state, or generated
artifacts. In particular, do not:

- edit, format, generate, patch, delete, or create files in the source
  repository;
- run dependency installation, builds, tests, code generation, or cleanup from
  the source repository;
- run `git fetch`, `pull`, `switch`, `checkout`, `reset`, `clean`, `add`,
  `commit`, `stash`, `merge`, `rebase`, or `push`;
- create branches, tags, commits, worktrees, submodules, or remotes;
- use the source repository as the working directory for extraction scripts.

Before reading, capture:

```text
git rev-parse HEAD
git status --porcelain=v1 --untracked-files=all
```

Repeat both commands after extraction and compare their complete outputs. Any
difference is a hard stop: report it and do not attempt to repair, reset, clean,
stash, or otherwise modify the source repository. Existing differences are
user-owned and must be preserved. Extract committed content from the pinned
commit with `git show` or `git archive`, not from modified working-tree files.

All copied files must enter a temporary directory outside the source repository
before sanitization, formatting, dependency installation, generation, builds, or
tests begin. All project writes occur in this repository.

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
- At least two independent external applications complete an integration before
  shared-core or stable direct-native work is approved.

## 4. Milestone map

| Milestone                     | Result                                          | Blocking gate                              |
| ----------------------------- | ----------------------------------------------- | ------------------------------------------ |
| M0 — Bootstrap                | Private planning repository and controls        | Scope and provenance reviewed              |
| M1 — Allowlisted extraction   | Compiling wallet runtime copied without history | Technical source/dependency audit complete |
| M2 — Wallet-core sanitization | Private features physically absent              | Forbidden-content and wallet tests pass    |
| M3 — React Native API         | Typed SDK, standard transport, and example      | Example works against mocked services      |
| M4 — Native distribution      | Prebuilt iOS and Android payloads in npm        | Clean consumer builds without Rust         |
| M5 — Security and CI          | Auditable pull-request and release pipelines    | All required checks enforced               |
| M6 — Migration and release    | Destination RC followed by public alpha         | Distribution decision and release approval |
| M7 — External validation      | Evidence from real third-party integrations     | Demand gate measured and documented        |
| M8 — Demand-led evolution     | Shared core or native APIs only when justified  | Separate design review for each expansion  |

Milestones are sequential. Work inside a milestone may run in parallel, but no
later milestone may weaken an earlier security or isolation gate.

## 5. Detailed work breakdown

### M0 — Repository bootstrap and project controls

Status: complete.

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
   - one maintainer approval for ordinary contributor changes;
   - repository-owner merge authority for completed agent-authored pull requests
     after primary review and passing required checks.
8. Add `CODEOWNERS` for Rust, React Native, native packaging, security docs, and
   workflows when maintainers are assigned.
9. Keep repository-authored bootstrap material under the existing provisional
   `MIT OR Apache-2.0` terms. Preserve the assessed component's MIT declarations
   and provenance during private implementation. Defer the extracted
   implementation's final copyright, attribution, and distribution decision to
   the destination-repository migration before publication.
10. Add the pinned code-quality toolchain and required `npm run quality` command
    described in section 6 before importing runtime source.

Exit criteria:

- Repository visibility is private.
- Scope, provenance, and roadmap are committed.
- No runtime source has been copied.
- Branch controls are ready before the source-import pull request.
- Markdown, workflow, shell, and source-policy checks run locally and in CI.
- Final extracted-source licensing remains an explicit destination-migration
  release gate rather than an internal source-import blocker.

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
4. Read the source repository under the section 2.1 contract and use
   `git archive` or `git show` at the pinned commit to copy allowlisted files
   into a temporary staging directory outside that repository.
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
- The working 1AM repository has exactly the same HEAD and complete porcelain
  status output captured before extraction.
- Imported handwritten source satisfies the quality policy or has a time-bounded
  reviewed exception.

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
5. Require all service URLs and credentials from the adopter. Do not provide 1AM
   defaults.
6. Provide only an in-memory checkpoint adapter for examples and tests.
7. Build an Expo example that demonstrates:
   - runtime creation and disposal;
   - wallet session open and close;
   - normal sync and progress;
   - snapshot and balances;
   - checkpoint round-trip;
   - transaction/proof/submission flow;
   - cancellation and recoverable errors.
8. Use mock services by default. Live preview-network tests require explicit CI
   secrets and never run for untrusted pull requests.

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

1. Run the root `npm run quality` command without warnings or skipped checks.
2. Run Rust host tests and TypeScript Jest/API declaration tests with the
   coverage thresholds defined in section 6.
3. Verify generated bindings are reproducible.
4. Build Android native libraries and the Android release example.
5. Build the iOS simulator framework and example.
6. Run secret, dependency, vulnerability, license, and forbidden-content scans.
7. Verify the npm pack list and report package sizes.

Release CI:

1. Rebuild all native artifacts from a clean tagged checkout.
2. Verify that package version, Git tag, generated bindings, and compatibility
   metadata agree.
3. Produce the npm tarball, native archives, SHA-256 manifest, SBOM, license
   report, and build provenance.
4. Re-run clean consumer builds from the packed tarball.
5. Require manual approval before npm publication or repository visibility
   changes.
6. Publish npm with trusted provenance and create matching GitHub release notes.

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

### M6 — Destination migration and `0.1.0-alpha.1` release

1. Freeze the implementation release-candidate commit.
2. Migrate the completed reviewed implementation to its destination repository
   while preserving this repository's commit provenance and without importing
   the 1AM source history.
3. In the destination repository, resolve and record the final copyright owner,
   attribution notices, package license expression, source distribution terms,
   and binary redistribution obligations.
4. Run the complete release pipeline without publishing.
5. Review the source tree, npm pack list, binary string scan, SBOM, dependency
   licenses, artifact sizes, and test results.
6. Install the candidate tarball in a separate clean Expo application.
7. Run a controlled preview-network smoke test:
   - open and sync a disposable wallet;
   - export and restore its checkpoint;
   - build and prove a transaction;
   - submit or intentionally cancel it;
   - confirm accepted, rejected, and ambiguous submission handling.
8. Verify no production or maintainer wallet material entered logs or artifacts.
9. Tag `v0.1.0-alpha.1` in the destination repository.
10. Publish the npm package with provenance and the matching GitHub release.
11. Change the destination repository visibility to public only after
    publication approval, the final copyright/distribution record, and a final
    secret scan.
12. Monitor install failures, native crashes, package size, documentation gaps,
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

- At least two independent applications complete an integration, or a documented
  external requirement justifies the next investment.
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

## 6. Code quality standard

Establish the quality toolchain during M0, before runtime source is imported.
Pin tool versions and commit the root `package-lock.json` so local and CI
results are identical.

### 6.1 Required tools

Use Node.js 22 and a private root `package.json` for repository tooling. Add
these tools and configurations:

- `prettier` for Markdown, JSON, YAML, JavaScript, and TypeScript formatting;
- `markdownlint-cli2` for all maintained Markdown;
- ESLint 9 flat configuration with type-aware `typescript-eslint`,
  `eslint-config-expo`, and React Hooks rules;
- TypeScript compiler checks with the strict options below;
- `rustfmt` and `cargo clippy` for all Rust targets and features;
- `actionlint` for GitHub Actions;
- ShellCheck for maintained shell scripts;
- `cargo llvm-cov` and Jest coverage reporting;
- `scripts/check-source-policy.mjs` for file-size and suppression policy.

Expose one stable command surface:

```json
{
  "scripts": {
    "format": "prettier --write .",
    "format:check": "prettier --check .",
    "lint:markdown": "markdownlint-cli2 \"**/*.md\"",
    "lint:ts": "eslint . --max-warnings 0",
    "lint:rust": "node scripts/run-rust-quality.mjs",
    "lint:workflows": "node scripts/run-actionlint.mjs",
    "lint:shell": "node scripts/run-shellcheck.mjs",
    "check:source-policy": "node scripts/check-source-policy.mjs",
    "typecheck": "node scripts/run-typecheck.mjs",
    "test": "node scripts/run-tests.mjs",
    "quality": "npm run format:check && npm run lint:markdown && npm run lint:ts && npm run lint:rust && npm run lint:workflows && npm run lint:shell && npm run check:source-policy && npm run typecheck && npm test"
  }
}
```

The wrappers must execute the exact underlying tools described above when their
workstream exists. Before a workstream is added, its wrapper prints an explicit
`not applicable: no matching files` result and exits successfully. A wrapper
must fail if matching files exist but the required configuration or executable
is missing. This keeps `npm run quality` stable from the documentation-only
bootstrap through the complete SDK without silently skipping relevant checks.

### 6.2 Source-size and complexity limits

Implement `scripts/check-source-policy.mjs` with these rules:

1. Handwritten production source files may not exceed 500 physical lines.
2. TypeScript/JavaScript functions may not exceed 80 logical lines, ignoring
   blank lines and comments.
3. TypeScript cyclomatic complexity may not exceed 15 and nesting depth may not
   exceed four.
4. Rust functions trigger an error through clippy's `too_many_lines` lint above
   80 lines. Keep `too_many_arguments` enabled.
5. Split files by responsibility; do not satisfy limits through compressed
   formatting, multiple statements per line, or moving code into an unrelated
   utility module.

Apply the 500-line rule to `.rs`, `.ts`, `.tsx`, `.js`, `.mjs`, `.swift`, `.kt`,
and maintained Gradle source. Exempt only:

- generated UniFFI, Swift, and Kotlin bindings;
- lockfiles;
- vendored third-party source;
- machine-generated fixtures and snapshots;
- generated release manifests.

Every exemption must be listed in `scripts/source-policy-exceptions.json` with:

- exact repository-relative path;
- rule being waived;
- concrete justification;
- tracking issue;
- expiry milestone or removal condition.

Glob exemptions are forbidden. CI fails for stale exception paths, missing
tracking issues, or exemptions whose removal condition has passed.

### 6.3 TypeScript rules

Enable these compiler options:

```json
{
  "compilerOptions": {
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "exactOptionalPropertyTypes": true,
    "noImplicitOverride": true,
    "noFallthroughCasesInSwitch": true,
    "noImplicitReturns": true,
    "useUnknownInCatchVariables": true
  }
}
```

Treat these as errors:

- explicit `any`, unsafe assignment, and unsafe member access;
- unhandled or floating promises;
- misused promises in callbacks;
- non-exhaustive command/result switches;
- unnecessary conditions and non-null assertions;
- unused imports, variables, and disable directives;
- React Hooks dependency or ordering violations;
- imports that cross documented package boundaries.

Use `unknown` only at external/native decoding boundaries and narrow it before
returning. Public APIs must expose concrete types.

### 6.4 Rust rules

Run `rustfmt --check` and clippy for the workspace, all targets, and all
features. Deny warnings and enable:

- `clippy::unwrap_used`;
- `clippy::expect_used`;
- `clippy::panic`;
- `clippy::todo`;
- `clippy::unimplemented`;
- `clippy::dbg_macro`;
- `clippy::print_stdout`;
- `clippy::print_stderr`;
- `unsafe_op_in_unsafe_fn`.

Tests may use `unwrap` or `expect` when failure text improves the test, but
production library paths may not. Limit handwritten `unsafe` code to the FFI
boundary and require a `SAFETY:` comment stating every invariant immediately
above each unsafe block.

### 6.5 Suppressions and technical debt

- Do not use file-wide `eslint-disable`, `@ts-ignore`, blanket Rust
  `#![allow(...)]`, or configuration-wide warning suppression.
- Use `@ts-expect-error` only with a description explaining the expected
  compiler error.
- Use the narrowest possible ESLint suppression with an adjacent explanation and
  tracking issue.
- Use Rust `#[expect(lint, reason = "...")]` on the smallest item possible.
- Generated files may contain generator-owned suppressions but must carry a
  generated-file marker and reproduce byte-for-byte in CI.
- `TODO` and `FIXME` comments require a GitHub issue reference.
- CI rejects unused suppressions and undocumented exceptions.

### 6.6 Tests and coverage

- Every behavior change requires a success test and the relevant error,
  cancellation, stale-handle, or malformed-input tests.
- Every bug fix requires a regression test that fails without the fix.
- TypeScript handwritten source must maintain at least 85% line, statement, and
  function coverage and 75% branch coverage.
- Rust handwritten runtime source must maintain at least 80% line coverage under
  `cargo llvm-cov`.
- Generated bindings, fixtures, examples, and FFI declaration glue are excluded
  from coverage calculations.
- Coverage may increase but may not drop below the recorded default-branch
  percentage, even when it remains above the numeric threshold.
- Security-sensitive parsing, checkpoint validation, signing transcripts,
  command decoding, and submission-state transitions require explicit positive
  and negative tests rather than relying only on aggregate coverage.

### 6.7 Review and architecture requirements

- Keep Rust free of network, persistent-storage, UI, and platform lifecycle I/O.
- Keep endpoint and credential configuration outside the runtime.
- Prefer dependency injection at I/O and time boundaries.
- Keep modules single-purpose and public APIs smaller than their internal
  implementation surfaces.
- Update API documentation, examples, and compatibility notes in the same pull
  request as a public behavior change.
- Do not merge commented-out code, unexplained magic values, hidden fallback
  behavior, or logging that may contain seeds, checkpoints, signatures, or
  transaction payloads.
- A pull request is not complete while a required quality check is skipped,
  flaky, warning-only, or disabled.

## 7. Issue breakdown

Create implementation issues in this order:

1. `M0: configure branch protection and repository templates`
2. `M0: add pinned linters, formatters, and source-quality policy`
3. `M0: record source-license evidence and defer the final decision to the destination migration`
4. `M1: define the source extraction allowlist`
5. `M1: inventory dependencies and redistribution obligations`
6. `M1: import the compiling wallet runtime snapshot`
7. `M1: reproduce UniFFI bindings in the new workspace`
8. `M2: remove social and private identity capabilities`
9. `M2: remove FT/NFT and verifier capabilities`
10. `M2: remove gateway authentication, IPFS, and private fast sync`
11. `M2: introduce the typed Rust wallet-core command enum`
12. `M2: add typed TypeScript command result mapping`
13. `M2: harden domain-separated signing and opaque checkpoints`
14. `M2: add forbidden-content scans`
15. `M3: package the Expo module and public TypeScript API`
16. `M3: implement the standard indexer/proof/node transport`
17. `M3: add injected logging and checkpoint host interfaces`
18. `M3: build the mocked clean Expo example`
19. `M4: produce and validate the dynamic Apple XCFramework`
20. `M4: produce and validate Android native libraries`
21. `M4: assemble prebuilt binaries into the npm tarball`
22. `M5: add pull-request CI and security scans`
23. `M5: add release CI, SBOM, checksums, and provenance`
24. `M5: complete public API, architecture, and security documentation`
25. `M6: migrate the completed implementation to the destination repository`
26. `M6: resolve final copyright, attribution, and distribution terms`
27. `M6: run the private alpha release candidate`
28. `M6: publish 0.1.0-alpha.1 and make the destination repository public`
29. `M7: onboard and document the first external integration`

An issue may be split into smaller pull requests, but its exit criteria must
remain intact.

## 8. Branching and release policy

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

## 9. Risk register

| Risk                                                    | Mitigation                                                            | Release blocker |
| ------------------------------------------------------- | --------------------------------------------------------------------- | --------------- |
| Private feature or credential leaks into public history | Allowlist copy, fresh history, secret and forbidden scans             | Yes             |
| Extracted code cannot build independently               | Compile before sanitization; preserve pinned revisions initially      | Yes             |
| Static Apple archive makes npm impractical              | Dynamic XCFramework and enforced size budget                          | Yes             |
| Consumers unexpectedly compile Rust                     | Package prebuilt binaries; clean build with Rust absent               | Yes             |
| Checkpoints expose wallet activity                      | Opaque format, no default persistence, production encryption contract | Yes             |
| Seed copies survive across FFI boundaries               | Threat model, best-effort wiping, caller ownership documentation      | Yes             |
| Transport failures misreport submission status          | Explicit `statusUnknown` and end-to-end failure tests                 | Yes             |
| Alpha diverges from the 1AM implementation              | Manual provenance-tracked ports; revisit only after demand gate       | No              |
| Upstream Ledger change breaks wire compatibility        | Exact pinning and per-release compatibility matrix                    | Yes             |
| Generated bindings become accidental stable APIs        | Mark internal and postpone direct-native distribution                 | No              |
| Quality rules are bypassed to accelerate extraction     | Required checks, explicit exceptions, and no warning-only gates       | Yes             |

## 10. Definition of done for every milestone

A milestone is complete only when:

1. Its deliverables and exit criteria are satisfied.
2. Tests and security checks are automated where repeatable.
3. Documentation describes the resulting behavior and limitations.
4. No unrelated scope expansion was introduced.
5. Generated and packaged artifacts were inspected, not merely built.
6. The working 1AM repository remains unchanged.
7. Follow-up risks and deferred work are recorded as issues rather than hidden
   in implementation notes.
8. Formatting, linting, source-size, type, test, and coverage gates pass without
   undocumented suppression.

## 11. Current next action

M0 repository controls and the pinned quality toolchain are complete. The
reviewed M1 extraction allowlist and dependency inventory are enforced by the
repository quality gate. Begin the sanitized M1 source extraction from the
pinned commit under a fresh section 2.1 source-state bracket. Final copyright,
attribution, and distribution terms remain deferred to the destination
repository and do not block private internal implementation.
