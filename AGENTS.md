# Repository Guidelines

## Project Structure & Module Organization

This is a planning-only bootstrap; implementation has not started. Read
`PLAN.md` for scope, `docs/LONG_TERM_PROJECT_PLAN.md` for delivery gates, and
`PROVENANCE.md` before importing code. The intended layout is:

- `crates/runtime/`: Rust wallet runtime and UniFFI boundary.
- `packages/react-native/`: public React Native/Expo package.
- `examples/expo/`: integration example and release smoke test.
- `docs/`: architecture, security, compatibility, and releases.
- `scripts/` and `.github/`: quality tooling, packaging, and CI.

Keep network, storage, UI, and lifecycle I/O out of Rust. Put those concerns
behind injected TypeScript host interfaces.

## Build, Test, and Development Commands

There is no executable toolchain yet. Build and test commands become valid after
M0 adds the root `package.json` and lockfile:

- `npm ci`: install the pinned Node.js 22 tooling.
- `npm run format`: apply Prettier formatting.
- `npm run quality`: run all formatting, lint, source-policy, type, and test
  gates.
- `npm test`: run the repository test wrapper.

For current documentation changes, run `git diff --check`.

## Coding Style & Naming Conventions

Use Prettier for web files and `rustfmt` for Rust. TypeScript must be strict,
avoid `any`, and narrow `unknown` at decoding boundaries. Rust production code
must avoid `unwrap`, `expect`, and undocumented `unsafe`; place a `SAFETY:`
invariant above each unsafe block. Keep handwritten source under 500 physical
lines. Use `camelCase` for functions, `PascalCase` for exported types and React
components, and `snake_case` for Rust modules.

## Testing Guidelines

Every behavior change needs a success case and relevant failure, cancellation,
or malformed-input coverage. Bug fixes require a regression test. Name
TypeScript tests `*.test.ts` or `*.test.tsx`; keep Rust unit tests near the
module and integration tests under `crates/runtime/tests/`. Targets are 85%
TypeScript line/statement/function coverage, 75% branch coverage, and 80% Rust
line coverage. Use only deterministic synthetic fixtures.

## Commit Guidelines

Work directly on `main`. Use the repository's Conventional Commit format, such
as `docs: define code quality gates` or `feat: add wallet sync`. Include a
commit description explaining what changed and why so the history is easy to
follow. Commit each feature after its implementation is complete and its
relevant checks and tests pass.

## Agent Workflow

For coding tasks, delegate implementation to Luna sub-agents using `x-high`
reasoning. The primary agent must review their work, run the relevant quality
checks and tests, and correct anything incomplete or incorrect before committing
the finished feature.

The primary agent is also authorized to complete the pull-request lifecycle for
work within the user's requested scope. After implementation and review are
complete and all required checks pass, mark a draft ready, accept or approve the
pull request where GitHub permits, merge it, and update the local target branch.
Do not leave completed work unmerged merely because the user did not separately
request the merge. Never bypass required checks or unresolved review
requirements, and do not merge unrelated or third-party changes without explicit
authorization.
