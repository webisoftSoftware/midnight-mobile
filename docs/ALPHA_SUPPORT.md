# Alpha limitations, support, and upgrades

`@1am/midnight-mobile@0.1.0-alpha.1` is an experimental, community-maintained
React Native wallet-core SDK. It is not published yet, is not an official
Midnight SDK, and is not approved for production or mainnet funds.

## Current limitations

### Release and assurance

- M5 prepares auditable CI and release evidence; M6 still must migrate the
  candidate to its destination repository and resolve final copyright,
  attribution, license, notice, and distribution terms.
- No npm publication, public repository transition, or user outreach is
  authorized before that decision and release approval.
- The project has not completed an independent external security audit.
- Controlled live preview smoke testing is an M6 gate; required pull-request
  tests use deterministic synthetic services and material.

### Product scope

- Only wallet-core session, normal sync, checkpoint, transaction, DUST, dApp,
  proof/balance, cancellation, and submission operations are included.
- Social, FT/NFT, verifier-artifact, gateway-authentication, private-fast-sync,
  viewing-key upload, IPFS, operated-service, application UI, telemetry, and
  app-specific storage features are intentionally absent.
- The SDK provides no indexer, proof server, node, URL, API key, credential,
  failover, or service-level agreement.
- Applications must implement their selected indexer's normal synchronization
  integration and inject the HTTP, WebSocket, authentication, storage, and
  lifecycle boundaries used by the standard transport.

### Security and storage

- Seeds cross JavaScript, Swift/Kotlin, UniFFI, and Rust; wiping is best effort.
- The package provides no mnemonic vault, hardware-wallet flow, secure heap,
  device-compromise detection, or remote attestation.
- No production checkpoint adapter is bundled. The in-memory store is for
  examples/tests only.
- Checkpoint checksums detect corruption, not tampering, disclosure, or
  rollback. Production storage requires adopter-owned authenticated,
  device-protected encryption.
- `pause()` does not close native sessions or evict their retained seeds.

### API and runtime

- At most two sessions may be open and one operation may be active per session.
- Generated Swift/Kotlin and UniFFI interfaces are unsupported implementation
  details.
- There is no stable direct Swift, Kotlin, Rust, CocoaPods, SwiftPM, Maven, or
  crates.io product.
- Checkpoint format, TypeScript API, errors, native packaging, and compatibility
  may change between alphas.
- There is no legacy/plaintext checkpoint migration or promised downgrade path.
- Submission timeouts and connection failures are intentionally ambiguous and
  require application reconciliation.

### Platforms

- Only the exact Expo 55 / React Native 0.83 compatibility line is tested.
- Expo Go, web, SSR, CommonJS, Legacy Architecture, iOS before 15.1, Android
  before API 24, unsupported ABIs, and non-mobile Apple/Android platforms are
  outside the support matrix.
- M4 validates clean builds and archives; real-device launch and store review
  are not yet release evidence.

## Support expectations

Until publication, maintainers accept private implementation feedback but make
no release support commitment.

After an alpha is published:

- only the latest published alpha receives best-effort bug and security fixes;
- exact versions in the release's [compatibility matrix](./COMPATIBILITY.md) are
  the supported reproduction baseline;
- general issues and integration reports have no response-time or resolution
  SLA;
- vulnerability reports follow [`SECURITY.md`](../SECURITY.md), including its
  acknowledgement and initial-triage targets;
- support never requires a wallet seed, checkpoint, credential, private
  endpoint, transaction payload, or unredacted log; and
- maintainers may close requests outside wallet-core scope or the tested
  platform matrix.

An alpha may be deprecated immediately for a severe security, integrity,
licensing, or release-chain defect. Published npm versions and Git tags are
immutable and will not be silently replaced. A correction uses a new prerelease
version.

## Versioning policy

Alpha versions use Semantic Versioning prerelease identifiers, beginning with
`0.1.0-alpha.1`. While the API is below 1.0 and explicitly alpha:

- any new alpha may contain breaking TypeScript, checkpoint, native packaging,
  error, or operational changes;
- a patch-looking prerelease increment does not imply checkpoint or source
  compatibility;
- Ledger revision changes are reviewed and documented separately from feature
  changes when practical;
- generated native binding compatibility is never inferred from the npm version;
  and
- adopters must pin an exact version, not `latest`, a caret, a tilde, a tag
  range, or a Git branch.

Release notes and the compatibility record are authoritative for each version.

## Upgrade procedure

Evaluate every upgrade as a wallet-state migration:

1. Read the release notes, compatibility matrix, security notes, SBOM, license
   report, provenance, and checksums.
2. Confirm that the package's final distribution terms are acceptable.
3. Retain the prior encrypted checkpoint and application release until
   validation completes.
4. Build the new npm tarball into a clean Expo project with the exact tested
   peer versions and `npx expo prebuild --clean`.
5. Run the deterministic mocked lifecycle on iOS and Android.
6. Test an application upgrade and checkpoint restore using synthetic state.
7. If the release explicitly supports the prior checkpoint, test a disposable
   preview wallet and reconcile pending submissions before further commands.
8. Roll out gradually with redaction-safe build, crash, and error monitoring.
9. Keep a defined user-facing recovery path for `STATE_INCOMPATIBLE`.

Never rewrite a stored checkpoint in place before authenticated backup succeeds.
Never convert opaque bytes in application code.

## Downgrades and rollback

Application-binary rollback is not the same as wallet-state rollback. A newer
runtime may write a checkpoint an older runtime cannot read. This alpha promises
no downgrade conversion.

If an upgrade fails before using live material, return to the prior application
and its untouched prior encrypted checkpoint. After a newer runtime has opened
or persisted wallet state, follow release-specific guidance; do not force the
bytes into an older SDK or silently start empty.

For an ambiguous submission, reconcile the transaction hash before upgrading,
downgrading, restoring an older checkpoint, or constructing a replacement.

## Reporting a problem

For a normal bug or integration gap, provide:

- exact SDK version or commit;
- Expo, React Native, OS, device/architecture, Xcode or Android tool versions;
- network identifier and endpoint roles without URLs or credentials;
- minimal deterministic synthetic reproduction;
- stable `MidnightRuntimeError.code`; and
- redacted build output that contains no wallet or service material.

Use private vulnerability reporting for security issues. See
[`CONTRIBUTING.md`](../CONTRIBUTING.md) and the
[unofficial disclaimer](./UNOFFICIAL_PROJECT_DISCLAIMER.md).
