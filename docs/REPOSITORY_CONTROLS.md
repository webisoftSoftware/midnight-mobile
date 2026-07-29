# Repository Controls

This document records the M0 controls that combine committed repository files
with GitHub-hosted settings. On 2026-07-29, a read-only GitHub query confirmed:

- repository: `ADGLx/midnight-mobile`;
- visibility: private;
- default branch: `main`;
- bootstrap owner: `@ADGLx`.

The same audit confirmed that the milestone, workstream, platform, type,
security, blocked-work, and external-demand labels are present. Dependabot
vulnerability alerts and automated security updates are enabled. The current
GitHub API token cannot read or enable private vulnerability reporting: the
endpoint returns `404`, so that setting still requires verification by a
repository administrator in the GitHub web interface.

The settings below must be applied and verified in GitHub before the M1 source
import pull request. Committing this document or a workflow does not configure
server-side repository settings.

## Branch protection for `main`

Configure a ruleset or branch protection rule that:

- requires a pull request before merging;
- requires at least one approving review;
- dismisses stale approvals when reviewable content changes;
- requires review from Code Owners;
- requires the `Repository quality` status check;
- requires branches to be up to date before merging;
- requires all review conversations to be resolved;
- blocks force pushes and branch deletion;
- prevents bypass for changes to code, native binaries, security policy, or
  release workflows.

The quality workflow deliberately gives its required job the stable name
`Repository quality`. Do not mark that check optional, warning-only, or
skippable when applicable.

## Ownership

`.github/CODEOWNERS` routes all paths to `@ADGLx` during bootstrap. This is a
temporary maintainer assignment, not evidence that specialist or intellectual
property ownership has been audited. Add or replace owners when the following
roles are assigned:

- Rust wallet runtime and UniFFI;
- public React Native and Expo API;
- Apple and Android native packaging;
- security policy and threat-model documentation;
- CI, binary builds, and release workflows.

Keep at least one owner with write access on every listed path. Complete the
separate ownership and dual-license audit before importing source.

## Labels

Create these labels in GitHub. Issue forms reference `type: bug`,
`type: feature`, and `external-demand`, while Dependabot references
`dependencies` and `workstream: quality`.

| Category         | Labels                                                                                                                                                                                                                                                                                                   |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Milestone        | `milestone: m0` through `milestone: m8`                                                                                                                                                                                                                                                                  |
| Workstream       | `workstream: bootstrap`, `workstream: source-import`, `workstream: runtime`, `workstream: react-native`, `workstream: transport`, `workstream: apple`, `workstream: android`, `workstream: packaging`, `workstream: quality`, `workstream: security`, `workstream: documentation`, `workstream: release` |
| Platform         | `platform: rust`, `platform: react-native`, `platform: expo`, `platform: ios`, `platform: android`, `platform: ci`                                                                                                                                                                                       |
| Type             | `type: bug`, `type: feature`, `type: integration`, `dependencies`                                                                                                                                                                                                                                        |
| State and demand | `security`, `blocked`, `external-demand`                                                                                                                                                                                                                                                                 |

Use the `security` label only for public, non-sensitive tracking. Vulnerability
details belong in a private security advisory.

## Security and dependency settings

- Enable GitHub private vulnerability reporting.
- Enable Dependabot alerts and security updates.
- Keep the repository private until the M6 publication gate and final secret
  scan have passed.
- Permit the pinned GitHub-owned actions used by the quality workflow.
- Review automated dependency pull requests with the same quality, provenance,
  compatibility, and licensing gates as other changes.
- Add Cargo Dependabot configuration only after `crates/runtime/Cargo.toml`
  exists; an invalid pre-declared directory would create noisy update failures.

## Verification record

Before source import, record evidence that:

1. the repository remains private and `main` remains the default branch;
2. branch protection or the equivalent ruleset is active;
3. the required check resolves to the `Repository quality` job;
4. CODEOWNERS review is requested on representative protected paths;
5. issue forms apply their configured labels;
6. private vulnerability reporting is reachable;
7. Dependabot alerts and security updates are enabled.
