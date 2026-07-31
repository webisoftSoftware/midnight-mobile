# Security Policy

Midnight Mobile handles security- and privacy-sensitive wallet operations.
Please report suspected vulnerabilities privately and minimize the sensitive
information included in every report.

## Supported versions

There is no published SDK version yet.

| Version             | Security support |
| ------------------- | ---------------- |
| `main` (unreleased) | Reports accepted |
| Published versions  | None yet         |

Support commitments for alpha releases will be stated in their release notes.
Until then, do not treat the project as production-ready.

After publication, only the latest alpha receives best-effort security fixes.
See the [alpha support policy](./docs/ALPHA_SUPPORT.md). A release is not
supported outside its exact [compatibility matrix](./docs/COMPATIBILITY.md).

## Report a vulnerability

While the repository remains private before the alpha release, contact the
repository owner through an established private channel and include only a
request to establish a secure reporting channel. After the repository becomes
public, use
[GitHub private vulnerability reporting](https://github.com/ADGLx/midnight-mobile/security/advisories/new).
Do not open a public issue, pull request, discussion, or integration report for
a suspected vulnerability.

Include only what maintainers need to reproduce and assess the issue:

- affected version or full commit hash;
- affected platform and environment;
- vulnerability class, impact, and required attacker capabilities;
- minimal deterministic reproduction using synthetic material;
- suggested mitigation, if known;
- whether disclosure has occurred elsewhere.

Never submit a real wallet seed, checkpoint, signing key, credential, private
endpoint, transaction payload, production wallet identifier, or sensitive log.
Redact local paths and unrelated identifiers. If even a private report would
require sensitive material, first send a description without that material so
maintainers can arrange a safer method.

If private vulnerability reporting is unavailable after publication, use the
same private contact fallback and include only a request to establish a secure
reporting channel.

## What to expect

Maintainers aim to acknowledge a report within three business days and provide
an initial triage result within seven business days. Complex reports may take
longer. Maintainers will coordinate validation, remediation, release timing, and
disclosure with the reporter when practical.

Please allow reasonable time for a fix before public disclosure. A severe issue
may require keeping the repository private, delaying a release, or deprecating
an affected package version. Published packages and tags will not be silently
replaced.

## Scope

Reports are especially useful when they affect:

- seed or checkpoint confidentiality and lifecycle;
- signing-domain separation or transaction integrity;
- native FFI memory handling;
- command decoding or removed-capability reachability;
- resumable effects, cancellation, generation fencing, or submission state;
- endpoint, credential, storage, or logging boundaries;
- npm, native binary, build, CI, provenance, or release-chain integrity;
- exposure of excluded private features or potentially live secrets.

General support questions, non-security bugs, and feature requests should use
the corresponding issue form with synthetic, sanitized data.

The documented trust boundaries are part of the security contract:

- [architecture and resumable effects](./docs/ARCHITECTURE.md);
- [checkpoint authenticated-encryption requirements](./docs/CHECKPOINTS.md);
- [seed lifecycle threat model](./docs/SEED_THREAT_MODEL.md);
- [network and proof-server configuration](./docs/NETWORK_CONFIGURATION.md); and
- [binary provenance](./docs/BINARY_PROVENANCE.md).

A report that an implementation violates one of those guarantees is in scope. A
third-party endpoint outage, unsupported platform, rooted/jailbroken device, or
malicious code already executing inside the application process is not by itself
an SDK vulnerability, but a concrete boundary bypass or unsafe SDK behavior in
that environment may be.

## Good-faith research

Good-faith research must avoid privacy violations, data destruction, service
degradation, social engineering, and access to accounts or data you do not own
or have explicit permission to test. Stop testing if you encounter real wallet
material or credentials and report the finding without retaining or sharing the
sensitive value.

Do not test against public network infrastructure without the operator's
permission. Use local deterministic services and disposable synthetic wallet
material whenever possible.
