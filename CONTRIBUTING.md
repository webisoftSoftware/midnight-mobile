# Contributing to Midnight Mobile

Thank you for helping build the community-maintained Midnight Mobile SDK. The
project is implementing a security-sensitive wallet runtime in sequential
milestones. Contributions must preserve the scope, provenance, and release gates
in [`docs/LONG_TERM_PROJECT_PLAN.md`](./docs/LONG_TERM_PROJECT_PLAN.md).

## Before proposing a change

1. Read [`PLAN.md`](./PLAN.md), the long-term plan, and
   [`PROVENANCE.md`](./PROVENANCE.md).
2. Search existing issues and choose the matching issue form.
3. For substantial work, agree on scope and acceptance criteria in an issue
   before implementation.
4. Report vulnerabilities privately according to [`SECURITY.md`](./SECURITY.md).
   Do not open a public security issue.

Social features, FT/NFT capabilities, verifier artifacts, 1AM gateway
authentication, private fast sync, viewing-key upload, operated-service
defaults, and unrelated application code are outside the alpha scope. Deferred
features require the demand and design gates in the long-term plan.

## Source and data safety

Do not submit wallet seeds, checkpoints, credentials, private endpoints,
transaction payloads, sensitive logs, production wallet material, or potentially
live secrets. Tests, examples, screenshots, and reports must use deterministic
synthetic data.

Do not copy from the assessed 1AM repository unless the exact path is approved
in the extraction allowlist and its provenance and licensing obligations are
documented. Extraction is from the pinned commit recorded in `PROVENANCE.md`,
through a temporary staging directory, without importing Git history. Never copy
modified working-tree content or private features.

If you discover potentially live credentials or private capability during
review, stop, do not commit it, and use the private security process.

## Development workflow

Use a short-lived branch and keep each pull request focused on one reviewed
issue or milestone outcome. Once the M0 toolchain is present:

```sh
npm ci
npm run quality
```

The quality wrapper is the required local and CI gate. It must run every
applicable formatter, linter, source-policy check, type check, and test without
warnings or silent skips. A wrapper may report a workstream as not applicable
only when no matching files exist.

Security- or release-boundary changes must also run:

```sh
npm run check:security
npm run check:release
npm run release:evidence
```

`check:release` and `release:evidence` validate a candidate and generate ignored
local evidence; they do not authorize publication. Do not publish a package,
create a public release, change repository visibility, or contact prospective
users from a contribution branch. The tagged release workflow adds
`--tag "$GITHUB_REF_NAME"` so the candidate tag must point at the checked-out
commit.

Follow these repository conventions:

- Format web files with Prettier and Rust with `rustfmt`.
- Keep TypeScript strict, avoid `any`, and narrow `unknown` at decoding
  boundaries.
- Keep network, persistence, UI, and lifecycle I/O out of Rust.
- Avoid `unwrap`, `expect`, and undocumented `unsafe` in production Rust.
- Keep handwritten production files under 500 physical lines.
- Add success and relevant failure, cancellation, stale-handle, or
  malformed-input tests for behavior changes.
- Add a regression test for every bug fix.
- Update public documentation and examples with behavior changes.

Public behavior changes must update the applicable:

- [quick start](./docs/QUICK_START.md);
- [architecture](./docs/ARCHITECTURE.md);
- [API and error reference](./docs/API_REFERENCE.md);
- [network configuration](./docs/NETWORK_CONFIGURATION.md);
- [checkpoint](./docs/CHECKPOINTS.md) or
  [seed threat model](./docs/SEED_THREAT_MODEL.md);
- [compatibility matrix](./docs/COMPATIBILITY.md); and
- [alpha limitations and upgrade policy](./docs/ALPHA_SUPPORT.md).

Use Conventional Commit subjects such as `feat: add wallet sync` or
`docs: clarify checkpoint storage`. Commit descriptions should explain both what
changed and why.

## Pull requests

Complete every section of the pull request template. In particular:

- link the issue and milestone criteria;
- describe user impact and security or secret-handling impact;
- list exact test commands and platform results;
- identify generated artifacts and how they were reproduced;
- confirm that excluded private capabilities and sensitive material are absent;
- document provenance and redistribution permission for copied material.

Generated bindings and native artifacts require maintainer review. A pull
request is not complete while a required check is skipped, flaky, warning-only,
or disabled. Maintainers may ask for a smaller change when extraction,
sanitization, transport, native packaging, and release concerns are mixed
together.

Repository-authored bootstrap material remains under the existing provisional
terms. Preserve assessed-component declarations and provenance during private
implementation. The extracted implementation's final copyright, attribution,
license, notice, and distribution decision is deliberately deferred to the M6
destination-repository migration. Do not represent the provisional package
metadata as that final decision or distribute release artifacts before it is
recorded.

By participating, you agree to follow
[`CODE_OF_CONDUCT.md`](./CODE_OF_CONDUCT.md).
