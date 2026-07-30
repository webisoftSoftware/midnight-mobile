# Repository Guidelines

## Project Structure & Module Organization

Read `PLAN.md` for scope, `docs/LONG_TERM_PROJECT_PLAN.md` for delivery gates,
and `PROVENANCE.md` before changing imported code. The implemented layout is:

- `crates/runtime/`: Rust wallet runtime and UniFFI boundary.
- `packages/react-native/`: public React Native/Expo package.
- `examples/expo/`: integration example and release smoke test.
- `docs/`: architecture, security, compatibility, and releases.
- `scripts/` and `.github/`: quality tooling, packaging, and CI.

Keep network, storage, UI, and lifecycle I/O out of Rust. Put those concerns
behind injected TypeScript host interfaces.

## Build, Test, and Development Commands

- `npm ci`: install the pinned Node.js 22 tooling.
- `npm run format`: apply Prettier formatting.
- `npm run quality`: run the complete local gate, including native, packaging,
  reproducibility, security, and release qualification.
- `npm run quality:pr`: run the lightweight formatting, lint, policy, type,
  unit-test, and coverage gate used by pull requests.
- `npm test`: run the repository test wrapper.

For documentation-only changes, also run `git diff --check`.

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

Use a dedicated branch and pull request for implementation work. Use the
repository's Conventional Commit format, such as
`docs: define code quality gates` or `feat: add wallet sync`. Include a commit
description explaining what changed and why so the history is easy to follow.
Commit each feature after its implementation is complete and its relevant checks
and tests pass.

## Agent Workflow

For coding tasks, delegate implementation to Luna sub-agents using `x-high`
reasoning. The primary agent must review their work, run the relevant quality
checks and tests, and correct anything incomplete or incorrect before committing
the finished feature.

The primary agent is also authorized to complete the pull-request lifecycle for
work within the user's requested scope. After implementation and review are
complete, every milestone acceptance requires a recorded successful full local
`npm run quality`. From M5 onward, the lightweight GitHub `Repository quality`
and separate `Dependency security` checks must also pass. Full native and
release CI remains manual-only until the final release stage. Then mark a draft
ready, record acceptance or approval where GitHub permits, merge it, and update
the local target branch. When GitHub prohibits self-approval of an
agent-authored pull request, completed primary-agent review plus the applicable
passing gates counts as acceptance; use repository-owner or administrator merge
authority if available. Do not leave completed work unmerged merely because the
user did not separately request the merge. Never bypass a failing local or
required check, unresolved third-party feedback, or an independent approval that
the user explicitly asked to retain, and do not merge unrelated or third-party
changes without explicit authorization.
