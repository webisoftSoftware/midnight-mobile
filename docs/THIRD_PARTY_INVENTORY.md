# Third-Party and Source-License Inventory

## Audit status

**Source landing is blocked.** The dependency licenses below are compatible in
principle with the planned distribution, but the assessed 1AM component's
ownership and source-license evidence is incomplete. No source-derived material
may enter this repository, a native binary, or an npm package until the source
license gap is resolved and reviewed.

This is an engineering compliance record, not legal advice. It inventories the
direct dependency surface at source commit
`dc9c0dd34dba9e19304859106ca2531f48590599`; it does not replace counsel or the
release-time transitive SBOM and license scan.

## Assessed source ownership and license

Evidence read from the pinned commit:

- `apps/mobile/modules/expo-midnight-native/package.json` declares
  `"license": "MIT"`.
- `apps/mobile/modules/expo-midnight-native/rust/Cargo.toml` declares workspace
  `license = "MIT"`; the runtime and bindgen manifests inherit it.
- `apps/mobile/modules/expo-midnight-native/ios/ExpoMidnightNative.podspec`
  inherits that package license and names `1AM` as author.
- The pinned repository archive contains no top-level `LICENSE`, `COPYING`, or
  `NOTICE` file.
- The assessed component contains no MIT license text and no per-file SPDX,
  copyright, or license headers. Its only license statements are the manifest
  metadata above.
- Commit author/committer metadata identifies Alvaro Gonzalez for the assessed
  commit. Commit metadata and a podspec author field do not establish ownership
  of every assessed file or authority to license all contributions.
- A 2026-07-29 GitHub commit-history query scoped to the assessed component and
  ending at the pinned revision returned 47 commits. Every result identifies
  GitHub account `ADGLx`, author name Alvaro Gonzalez, and the same author
  email; no second component contributor was reported.
- GitHub repository metadata identifies `webisoftSoftware` as the repository
  owner and reports no detected repository license. Repository ownership and
  sole Git authorship still do not establish whether copyright belongs to the
  individual, the `1AM` podspec author, Webisoft Software, an employer, or
  another commissioning party.

The manifest declarations are evidence of intended MIT licensing, but they do
not provide the required MIT notice or prove that the declarant owns or is
authorized to license every contribution. They are therefore insufficient for
the M0 ownership audit and for copying or redistributing source.

### Required evidence to clear the blocker

Record all of the following before source landing:

1. The legal name of the relevant copyright owner and confirmation of the
   relationship among Alvaro Gonzalez, `ADGLx`, `1AM`, and Webisoft Software.
2. Confirmation that the 47-commit component history correctly represents sole
   authorship and that no unlisted third-party source was incorporated into the
   allowlisted files.
3. Evidence that the owner or authorized licensor grants the assessed component
   under MIT at the pinned commit, including the exact copyright and permission
   notice to preserve.
4. Explicit authorization or review that the derivative may be distributed under
   `MIT OR Apache-2.0` while preserving the upstream MIT notice.
5. A durable copy of the evidence or an immutable reference recorded in
   `PROVENANCE.md`.

A later repository metadata change or license file must not silently change the
assessed revision. If the evidence grants a different revision or component,
update `PROVENANCE.md`, the extraction allowlist, this inventory, and the
compatibility record through review.

## Obligation codes

The tables use these conservative distribution requirements:

- **MIT** — include the dependency's copyright and MIT permission notice in all
  copies or substantial portions.
- **Apache-2.0** — provide the Apache 2.0 license, retain applicable copyright,
  patent, trademark, and attribution notices, mark modified source files, and
  carry forward an upstream `NOTICE` if one exists.
- **MPL-2.0** — include the MPL 2.0 notice; keep modifications to MPL-covered
  files under MPL 2.0; when distributing executable form, make the Source Code
  Form of the covered software available as required by MPL 2.0 and inform
  recipients how to obtain it. MPL does not require unrelated larger-work files
  to use MPL.
- **Build/test only** — do not ship the dependency in release artifacts. If it
  is later bundled, reclassify and perform its binary-distribution review.
- **Peer/reference only** — the npm package points to or expects the dependency
  but does not bundle it. If a release artifact vendors it, its license and
  notices become part of that artifact's obligations.

For `MIT OR Apache-2.0` dependencies, release engineering must make and record a
license choice or satisfy both sets of notices. This inventory does not treat
the project's generic license files as substitutes for dependency-specific
copyright notices.

## Direct Rust dependencies

Versions, checksums, and sources are fixed by the assessed runtime
`Cargo.toml`/`Cargo.lock`. Registry SPDX values were checked against the exact
crates.io version records.

| Dependency                  | Exact version/source                                  | Declared license    | Scope             | Distribution treatment                       |
| --------------------------- | ----------------------------------------------------- | ------------------- | ----------------- | -------------------------------------------- |
| `anyhow`                    | `1.0.102`                                             | `MIT OR Apache-2.0` | Runtime           | Select MIT or Apache-2.0; include notices.   |
| `bech32`                    | `0.11.1`                                              | `MIT`               | Runtime           | MIT notice.                                  |
| `futures`                   | `0.3.32`                                              | `MIT OR Apache-2.0` | Declared, unused  | Remove from the sanitized direct graph.      |
| `flate2`                    | `1.1.9`, `default-features = false`, `rust_backend`   | `MIT OR Apache-2.0` | Runtime           | Select MIT or Apache-2.0; include notices.   |
| `hex`                       | `0.4.3`                                               | `MIT OR Apache-2.0` | Runtime           | Select MIT or Apache-2.0; include notices.   |
| `num-bigint`                | `0.4.8`, `default-features = false`                   | `MIT OR Apache-2.0` | Runtime           | Select MIT or Apache-2.0; include notices.   |
| `rand`                      | `0.8.7`                                               | `MIT OR Apache-2.0` | Runtime           | Select MIT or Apache-2.0; include notices.   |
| `serde`                     | `1.0.228`, `derive`                                   | `MIT OR Apache-2.0` | Runtime           | Select MIT or Apache-2.0; include notices.   |
| `serde_json`                | `1.0.145`                                             | `MIT OR Apache-2.0` | Runtime           | Select MIT or Apache-2.0; include notices.   |
| `sha2`                      | `0.10.9`                                              | `MIT OR Apache-2.0` | Runtime           | Select MIT or Apache-2.0; include notices.   |
| `thiserror`                 | `2.0.17`                                              | `MIT OR Apache-2.0` | Runtime           | Select MIT or Apache-2.0; include notices.   |
| `uniffi`                    | `0.32.0`; runtime and bindgen (`cli`)                 | `MPL-2.0`           | Runtime and build | MPL-2.0 source/executable-form obligations.  |
| `zeroize`                   | `1.9.0`, `default-features = false`                   | `Apache-2.0 OR MIT` | Runtime           | Select Apache-2.0 or MIT; include notices.   |
| `proptest`                  | `1.11.0`                                              | `MIT OR Apache-2.0` | Test only         | Build/test only; do not place in binaries.   |
| `midnight-ledger`           | `8.1.0` at `02716c2c95d50654aeb3cb63bfd8386046e4ca7d` | `Apache-2.0`        | Runtime           | Apache-2.0; include license and attribution. |
| `midnight-onchain-runtime`  | `3.1.0` at `02716c2c95d50654aeb3cb63bfd8386046e4ca7d` | `Apache-2.0`        | Runtime           | Apache-2.0; include license and attribution. |
| `midnight-base-crypto`      | `1.0.0` at `02716c2c95d50654aeb3cb63bfd8386046e4ca7d` | `Apache-2.0`        | Runtime           | Apache-2.0; include license and attribution. |
| `midnight-coin-structure`   | `2.0.1` at `02716c2c95d50654aeb3cb63bfd8386046e4ca7d` | `Apache-2.0`        | Runtime           | Apache-2.0; include license and attribution. |
| `midnight-serialize`        | `1.1.0` at `02716c2c95d50654aeb3cb63bfd8386046e4ca7d` | `Apache-2.0`        | Runtime           | Apache-2.0; include license and attribution. |
| `midnight-storage`          | `2.0.1` at `02716c2c95d50654aeb3cb63bfd8386046e4ca7d` | `Apache-2.0`        | Runtime           | Apache-2.0; include license and attribution. |
| `midnight-transient-crypto` | `2.1.0` at `02716c2c95d50654aeb3cb63bfd8386046e4ca7d` | `Apache-2.0`        | Runtime           | Apache-2.0; include license and attribution. |
| `midnight-zswap`            | `8.1.0` at `02716c2c95d50654aeb3cb63bfd8386046e4ca7d` | `Apache-2.0`        | Runtime           | Apache-2.0; include license and attribution. |

The pinned Midnight Ledger workspace manifest declares
`workspace.package.license = "Apache-2.0"` and includes an Apache 2.0 `LICENSE`;
no `NOTICE` exists at that revision. The runtime uses eight direct workspace
crates, but the resulting native libraries also contain their transitive
dependency graph. A complete release SBOM and transitive license report remain
mandatory.

UniFFI is the only direct MPL dependency identified here. Before distributing
native binaries, record the exact UniFFI source archive/commit corresponding to
`0.32.0`, package the MPL text and source-availability notice, and verify
whether generated Swift/Kotlin templates carry any additional notices.

## Direct npm and Expo dependencies

The assessed nested module manifest declares **no** `dependencies`,
`peerDependencies`, or `devDependencies`. That is a manifest defect, not proof
that the module has no npm requirements:

- `native-boundary.ts` imports `expo-modules-core`;
- `MidnightRuntimeProvider.tsx` imports `react` and `react-native`;
- the podspec depends on `ExpoModulesCore` without a version constraint;
- the Android build applies `expo-module-gradle-plugin` without a version in the
  component.

The enclosing assessed mobile application resolves this exact compatibility set:

| Package             | Exact assessed resolution | Declared license | Intended SDK treatment                                                     |
| ------------------- | ------------------------- | ---------------- | -------------------------------------------------------------------------- |
| `expo`              | `55.0.23`                 | `MIT`            | Expo SDK compatibility peer/reference; not bundled.                        |
| `expo-modules-core` | `55.0.25`                 | `MIT`            | Direct runtime dependency and `ExpoModulesCore` pod basis; include notice. |
| `react`             | `19.2.0`                  | `MIT`            | Peer dependency; not bundled.                                              |
| `react-native`      | `0.83.6`                  | `MIT`            | Peer dependency; not bundled.                                              |

The target `@1am/midnight-mobile` manifest must explicitly declare and pin or
constrain these relationships before TypeScript source lands. The exact
CocoaPods constraint for `ExpoModulesCore` and the exact Gradle plugin
resolution are not stated by the assessed component; they remain evidence gaps
until the target package and clean Expo example resolve them reproducibly.

React test tooling, Jest, and TypeScript are supplied by the enclosing upstream
application rather than the assessed module. They are not direct runtime
dependencies for this inventory. The target package's eventual dev dependency
set must be inventoried from its own lockfile.

## Android/JNA dependency

The assessed Gradle file declares the exact coordinate:

```text
net.java.dev.jna:jna:5.17.0@aar
```

The Maven Central POM and the tagged upstream `LICENSE` offer
`LGPL-2.1-or-later OR Apache-2.0`. This project records **Apache-2.0** as the
intended choice. Do not apply LGPL treatment implicitly.

If Gradle resolves JNA rather than the npm tarball vendoring it, the SDK package
must retain the exact coordinate and the consuming application receives the AAR.
Any release smoke application or redistributed native aggregate that contains
JNA must include the JNA license/attribution, retain notices, and mark
modifications under Apache 2.0. The JNA 5.17.0 tag has no `NOTICE` file.

The AAR's checksum is not pinned in the assessed component. Capture the resolved
artifact checksum in the target dependency lock/verification metadata before the
Android binary gate.

## Source and binary release requirements

Before distributing source, generated bindings, native libraries, an example
binary, or an npm tarball:

1. Resolve the assessed source ownership/license blocker.
2. Generate a complete transitive SBOM for Cargo, npm, CocoaPods, Gradle, and
   native payloads; this direct inventory is not sufficient for release.
3. Generate a third-party notices artifact with dependency-specific copyright
   and license notices and include it in the npm tarball and GitHub release.
4. Include Apache 2.0 notices for Midnight Ledger and the selected JNA terms.
5. Satisfy UniFFI's MPL executable-form/source-availability requirements.
6. Verify the npm tarball does not bundle peer dependencies unexpectedly.
7. Verify generated bindings and binary strings contain no excluded features,
   credentials, upstream paths, or unreviewed notices.
8. Record exact checksums for the npm tarball, Apple/Android libraries, JNA AAR,
   SBOM, notices, and source archive.

`NOTICE` is not changed by this audit because no third-party source or binary is
currently distributed and the existing file already states that the ownership
audit is incomplete. Add verified upstream attribution to `NOTICE` only when the
source blocker is cleared and source-derived material is approved to land.

## Evidence references

- Assessed module
  [`package.json`](https://github.com/webisoftSoftware/one-am-wallet/blob/dc9c0dd34dba9e19304859106ca2531f48590599/apps/mobile/modules/expo-midnight-native/package.json)
  and
  [Rust workspace manifest](https://github.com/webisoftSoftware/one-am-wallet/blob/dc9c0dd34dba9e19304859106ca2531f48590599/apps/mobile/modules/expo-midnight-native/rust/Cargo.toml)
- Assessed
  [runtime manifest](https://github.com/webisoftSoftware/one-am-wallet/blob/dc9c0dd34dba9e19304859106ca2531f48590599/apps/mobile/modules/expo-midnight-native/rust/runtime/Cargo.toml),
  [Android Gradle file](https://github.com/webisoftSoftware/one-am-wallet/blob/dc9c0dd34dba9e19304859106ca2531f48590599/apps/mobile/modules/expo-midnight-native/android/build.gradle),
  and
  [podspec](https://github.com/webisoftSoftware/one-am-wallet/blob/dc9c0dd34dba9e19304859106ca2531f48590599/apps/mobile/modules/expo-midnight-native/ios/ExpoMidnightNative.podspec)
- Midnight Ledger
  [workspace manifest](https://github.com/midnightntwrk/midnight-ledger/blob/02716c2c95d50654aeb3cb63bfd8386046e4ca7d/Cargo.toml)
  and
  [Apache 2.0 license](https://github.com/midnightntwrk/midnight-ledger/blob/02716c2c95d50654aeb3cb63bfd8386046e4ca7d/LICENSE)
- UniFFI
  [0.32.0 manifest](https://github.com/mozilla/uniffi-rs/blob/v0.32.0/uniffi/Cargo.toml)
  and
  [MPL 2.0 license](https://github.com/mozilla/uniffi-rs/blob/v0.32.0/LICENSE)
- JNA
  [5.17.0 Maven POM](https://repo1.maven.org/maven2/net/java/dev/jna/jna/5.17.0/jna-5.17.0.pom)
  and
  [dual-license record](https://github.com/java-native-access/jna/blob/5.17.0/LICENSE)
- Exact crates.io version records:
  [`anyhow`](https://crates.io/api/v1/crates/anyhow/1.0.102),
  [`bech32`](https://crates.io/api/v1/crates/bech32/0.11.1),
  [`futures`](https://crates.io/api/v1/crates/futures/0.3.32),
  [`flate2`](https://crates.io/api/v1/crates/flate2/1.1.9),
  [`hex`](https://crates.io/api/v1/crates/hex/0.4.3),
  [`num-bigint`](https://crates.io/api/v1/crates/num-bigint/0.4.8),
  [`rand`](https://crates.io/api/v1/crates/rand/0.8.7),
  [`serde`](https://crates.io/api/v1/crates/serde/1.0.228),
  [`serde_json`](https://crates.io/api/v1/crates/serde_json/1.0.145),
  [`sha2`](https://crates.io/api/v1/crates/sha2/0.10.9),
  [`thiserror`](https://crates.io/api/v1/crates/thiserror/2.0.17),
  [`uniffi`](https://crates.io/api/v1/crates/uniffi/0.32.0),
  [`zeroize`](https://crates.io/api/v1/crates/zeroize/1.9.0), and
  [`proptest`](https://crates.io/api/v1/crates/proptest/1.11.0)
- Exact npm records:
  [`expo-modules-core@55.0.25`](https://registry.npmjs.org/expo-modules-core/55.0.25),
  [`react@19.2.0`](https://registry.npmjs.org/react/19.2.0), and
  [`react-native@0.83.6`](https://registry.npmjs.org/react-native/0.83.6)
