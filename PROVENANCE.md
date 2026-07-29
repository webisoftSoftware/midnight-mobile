# Provenance

This repository was bootstrapped as a fresh project on 2026-07-29.

The accepted extraction plan was produced from an assessment of:

- Source repository: `https://github.com/webisoftSoftware/one-am-wallet`
- Source commit: `dc9c0dd34dba9e19304859106ca2531f48590599`
- Assessed component: `apps/mobile/modules/expo-midnight-native`

No 1AM source code or Git history is included in the bootstrap commit. Future
source extraction must use
[`docs/EXTRACTION_ALLOWLIST.md`](./docs/EXTRACTION_ALLOWLIST.md) and the
sanitization gates in `PLAN.md`.

The source and dependency metadata at the assessed commit was audited on
2026-07-29. The component manifests claim MIT, but the assessed repository
contains no root license/notice file, the component contains no license text or
per-file license headers, and the available metadata does not conclusively
identify the copyright owner. By project-owner direction on 2026-07-29, that
question is deferred to the future destination-repository migration and is not
an internal source-landing gate in this private implementation repository. The
known evidence and the destination release gate are recorded in
[`docs/THIRD_PARTY_INVENTORY.md`](./docs/THIRD_PARTY_INVENTORY.md).

No npm publication, public repository transition, or other public distribution
may occur until the destination repository records the final copyright,
attribution, and distribution decision.

An initial 2026-07-29 extraction audit bracket did not pass because the source
checkout's complete porcelain output changed during the read-only audit. A fresh
audit was then started after capturing source-checkout HEAD
`33c1b1e7040714bfc37e812f0ac0d0fd1cf58eeb` and all 54 porcelain entries. The
full audit used only immutable objects at the assessed commit. Its final HEAD
and complete porcelain output were byte-identical to the fresh capture, and no
existing difference was inside the assessed component. The SHA-256 digest of the
exact porcelain output, including its trailing newline, was
`ca9a5dbf0172697e4f73367eb043f1d8bfd3f7a8bc25cd5f17e7e43ffa38029f`.

That successful audit validated a closed-world partition of all 110 files in the
assessed component: 69 candidate inputs, 39 exact exclusions, and two
component-level exclusions. A new pre/post capture is still required around any
future extraction. No source material was copied during either audit.

## M1 extraction record

The M1 extraction was completed on 2026-07-29 from immutable objects at the
assessed commit. An initial extraction bracket was abandoned when the source
checkout's porcelain output changed while the audit was in progress. None of
that checkout state was repaired or modified by this project.

The successful retry began and ended with source-checkout HEAD
`c4ac349ca8ddcc2188e5715507272decd4254c1b` and an empty complete
`git status --porcelain=v1 --untracked-files=all` output. The pre- and
post-extraction captures were byte-identical.

Exactly the 69 candidate paths recorded in
[`docs/EXTRACTION_ALLOWLIST.md`](./docs/EXTRACTION_ALLOWLIST.md) were archived
from source commit `dc9c0dd34dba9e19304859106ca2531f48590599`. The uncompressed
candidate archive had SHA-256 digest
`57f2cfd30d1d920ad3041a6db52bccbede90613930148e2b6e41fc5bb683fb74`. A second
independently bracketed archive was byte-identical. The archive was expanded
only in external temporary staging; denied source paths, Git history, and the
source working tree did not enter this repository.

Only reviewed, sanitized target files derived from those candidate inputs are
eligible to land here. The raw archive and temporary staging directory are audit
inputs, not repository or release artifacts. The exact landed Rust and React
Native path set and each file's origin class are machine-checked in
[`scripts/m1-sanitized-target-manifest.json`](./scripts/m1-sanitized-target-manifest.json).

That 72-file M1 manifest is immutable extraction evidence. M2
repository-authored hardening tests and test infrastructure are tracked
separately in the live 80-file
[`scripts/m2-sanitized-target-manifest.json`](./scripts/m2-sanitized-target-manifest.json).
The wallet-core boundary gate points to the M2 manifest without rewriting the M1
landing record.

This project is community-maintained and is not an official Midnight SDK.
