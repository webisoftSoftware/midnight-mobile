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

By participating, you agree to follow
[`CODE_OF_CONDUCT.md`](./CODE_OF_CONDUCT.md).
