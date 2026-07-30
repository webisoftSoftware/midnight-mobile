# Binary provenance and reproducible builds

Every released npm tarball and native archive must be traceable to one immutable
tag in the destination repository. The current private repository can build
release evidence, but publication is forbidden until the M6 destination
migration resolves copyright, attribution, licensing, notices, and binary
distribution terms.

## Source lineage

The lineage has three distinct boundaries:

1. [`PROVENANCE.md`](../PROVENANCE.md) identifies the assessed 1AM component at
   commit `dc9c0dd34dba9e19304859106ca2531f48590599`.
2. Immutable extraction manifests record the exact allowlisted source landing;
   later milestone manifests extend the sanitized target set without rewriting
   that evidence.
3. A release tag identifies the reviewed repository state from which package,
   binaries, SBOM, license report, checksums, and provenance are built.

The source 1AM Git history is not imported. The release must not be represented
as a build of the entire source repository or a moving upstream branch.

## Pinned inputs

Reproduction requires:

- Node.js 22 and npm `11.12.1`;
- the committed `package-lock.json`;
- Rust `1.97.1` and the root `rust-toolchain.toml`;
- Midnight Ledger 8.1.0 at `02716c2c95d50654aeb3cb63bfd8386046e4ca7d`;
- `cargo-llvm-cov 0.8.6`;
- Apple deployment target 15.1 and the three configured Rust targets;
- Android command-line tools `14742923`, NDK `27.1.12297006`, API 24, and the
  two configured Rust targets;
- JDK `17.0.19+10` for the Android consumer build;
- Xcode 16.4 and CocoaPods 1.16.2 for the M5 release baseline; and
- JNA `5.17.0@aar` with its pinned SHA-256.

The machine-readable native contract is
[`scripts/native-build-config.json`](../scripts/native-build-config.json). The
complete dependency evidence is in
[`THIRD_PARTY_INVENTORY.md`](./THIRD_PARTY_INVENTORY.md). Toolchain or
dependency drift requires a reviewed compatibility change; byte equality across
different compiler/SDK versions is not promised.

The historical local M4 acceptance run used Xcode 26.6 (`17F113`). It is local
M4 evidence only and does not replace the Xcode 16.4/CocoaPods 1.16.2 M5 release
contract.

## Rebuild from a tag

Use a fresh destination-repository checkout with no untracked or modified files.
Replace the example tag only with the exact candidate being verified:

```sh
git clone https://github.com/ADGLx/midnight-mobile.git
cd midnight-mobile
git checkout --detach v0.1.0-alpha.1
git status --short

npm ci
npm run quality
npm run check:security
npm run check:release -- --tag v0.1.0-alpha.1
node scripts/prepare-release-source.mjs --tag v0.1.0-alpha.1
npm run release:evidence
npm run release:verify
```

`git status --short` must be empty before building. Do not reproduce from a
source archive that omits submodule/dependency lock context, from a branch tip,
or with a regenerated lockfile.

The commands have separate purposes:

- `npm run quality` rebuilds native/package outputs, runs the full formatting,
  lint, source-boundary, declaration, clean-consumer, type, test, and coverage
  gate;
- `npm run check:security` checks the repository, full Git history, and packed
  npm tarball for secrets; verifies dependency origins, integrity, licenses, and
  reviewed patched source; and rejects forbidden content;
- the dependency-security workflow downloads the public OSV and RustSec
  databases, then scans the npm and Cargo lockfiles locally without transmitting
  the project dependency graph;
- `npm run check:release -- --tag ...` verifies tag/package/compatibility
  agreement and release prerequisites without publishing; and
- `prepare-release-source.mjs` archives the exact tag and records immutable tag
  metadata;
- `npm run release:evidence` deterministically writes the reviewed release
  evidence; and
- `npm run release:verify` checks every final manifest subject's path, size, and
  SHA-256.

The tagged workflow performs the same quality and release checks from a clean
checkout, then waits for the protected `alpha-release` environment before any
publication step. M5 automation must not publish while the M6 distribution
decision is absent. `docs/DISTRIBUTION_DECISION.json` is intentionally absent
until the M6 destination-repository review records an approved decision for the
exact package version.

## Expected outputs

Native/package artifacts are ignored by Git and generated under:

| Path                                                         | Purpose                                       |
| ------------------------------------------------------------ | --------------------------------------------- |
| `artifacts/apple/MidnightNativeRuntime.xcframework.zip`      | Standalone Apple XCFramework archive          |
| `artifacts/native/android-jniLibs.zip`                       | Standalone Android `jniLibs` archive          |
| `artifacts/native/npm/1am-midnight-mobile-0.1.0-alpha.1.tgz` | Final npm package candidate                   |
| `artifacts/native/SHA256SUMS.json`                           | Binary, archive, and tarball hashes and sizes |
| `artifacts/native/android/android-binaries.json`             | Android ABI, target, size, and hash report    |

Exact tagged-source inputs are generated under `artifacts/release-inputs/`:

| Path                                    | Purpose                                                           |
| --------------------------------------- | ----------------------------------------------------------------- |
| `midnight-mobile-v0.1.0-alpha.1.tar.gz` | Exact `git archive` of the tagged source                          |
| `tag-metadata.json`                     | Tag, commit, timestamp, package, compatibility, and source digest |
| `SHA256SUMS.json`                       | Source-archive and metadata hashes and sizes                      |

M5 release evidence is generated under `artifacts/release/`:

| Path              | Purpose                                                                        |
| ----------------- | ------------------------------------------------------------------------------ |
| `sbom.cdx.json`   | CycloneDX dependency and component inventory                                   |
| `licenses.json`   | Declared dependency license inventory and deferred distribution status         |
| `provenance.json` | Source tag/commit, toolchain, and artifact-subject record                      |
| `SHA256SUMS.json` | Hashes/sizes for source, tag, native, package, and generated evidence subjects |

The matching GitHub release must attach the npm tarball, both native archives,
exact source archive/tag metadata, release checksum manifest, SBOM, license
report, and provenance record produced in the same approved run. The final
release manifest covers the source archive, tag metadata, native archives,
tarball, SBOM, license report, and local provenance. Do not mix artifacts across
runs or tags.

The local `provenance.json` is deterministic, SLSA-shaped evidence. It is not
signed and must not be described as an attestation. The release workflow
separately creates a signed GitHub artifact attestation for the approved release
subjects and a signed GitHub SBOM attestation. Verify all three: the local
evidence explains the build inputs, the artifact attestation binds released
bytes to the workflow identity, and the SBOM attestation binds the reviewed
component inventory to that same release run.

## Reproducibility checks

The gates build and inspect, rather than merely package:

- Apple target libraries are built in isolated passes; framework trees and
  normalized archives compare byte-for-byte;
- Android target libraries are built twice and compare by SHA-256;
- generated Swift and Kotlin bindings reproduce exactly and expose only the
  expected eight-function ABI;
- package JavaScript/declarations and the npm tarball reproduce byte-for-byte;
- Apple Mach-O slices, deployment target, install name, dependencies,
  architectures, embedding, and signatures are inspected;
- Android ELF machines, API/NDK contract, dependencies, symbols, and APK ABIs
  are inspected;
- source, declarations, bindings, binaries, archives, and the tarball are
  scanned for forbidden capabilities and secrets; and
- clean Expo consumers install the tarball and build release variants with
  `cargo`, `rustc`, and `rustup` unavailable.

The current size limits are 15 MiB for the compressed Apple archive and 30 MiB
combined for Android runtime libraries.

## Verify downloaded artifacts

Obtain `artifacts/release/SHA256SUMS.json` through the same GitHub release as
the artifact. Confirm:

1. the release tag and commit match the provenance record;
2. each source archive, tag metadata, native archive, npm tarball, SBOM, license
   report, and provenance subject appears exactly once in the release manifest;
3. local byte size equals the manifest size;
4. local SHA-256 equals the manifest digest;
5. the SBOM, license report, and provenance files are themselves covered by the
   release checksum record; and
6. the checksum manifests are included in the signed GitHub artifact
   attestation; and
7. package version, tag, compatibility record, native reports, and tarball
   metadata all identify the same release.

On macOS, a single digest can be computed with:

```sh
shasum -a 256 1am-midnight-mobile-0.1.0-alpha.1.tgz
```

On platforms with GNU coreutils:

```sh
sha256sum 1am-midnight-mobile-0.1.0-alpha.1.tgz
```

Do not install when any digest, size, subject, tag, or provenance field differs.

## What reproducible means

The project claims byte-for-byte reproducibility only under the pinned inputs
and normalized build paths exercised by its gates. It does not claim:

- independent reproducibility under different Xcode, NDK, Rust, npm, or OS
  versions;
- reproducibility of code signing performed with an adopter identity;
- that a checksum proves source safety or correct licensing;
- that the assessed 1AM repository and this sanitized runtime are identical; or
- that generated artifacts may be distributed before the M6 decision.

Checksums establish byte identity. Review, security scans, SBOM, provenance,
compatibility, and a resolved distribution decision are separate release gates.
