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

This project is community-maintained and is not an official Midnight SDK.
