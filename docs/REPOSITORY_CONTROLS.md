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
repository is private, so GitHub does not offer private vulnerability reporting
for it.
[GitHub limits that feature](https://docs.github.com/en/code-security/how-tos/report-and-fix-vulnerabilities/configure-vulnerability-reporting/configure-for-a-repository)
to public repositories. Enable and verify it during the M6 visibility change;
until then, `SECURITY.md` provides the private pre-release contact path.

Except for controls explicitly assigned to the M6 visibility change, the
settings below must be applied and verified in GitHub before the M1 source
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
- permits repository-owner or administrator merge authority for completed
  agent-authored pull requests after primary review and passing required checks,
  as defined in `AGENTS.md`;
- never permits bypass of a failing required check, unresolved third-party
  feedback, or an independent approval the user explicitly required.

The quality workflow deliberately gives its required job the stable name
`Repository quality`. Do not mark that check optional, warning-only, or
skippable when applicable.

## Ownership

`.github/CODEOWNERS` routes all paths to `@ADGLx` during bootstrap. This is a
temporary maintainer assignment, not evidence of specialist ownership. Add or
replace owners when the following roles are assigned:

- Rust wallet runtime and UniFFI;
- public React Native and Expo API;
- Apple and Android native packaging;
- security policy and threat-model documentation;
- CI, binary builds, and release workflows.

Keep at least one owner with write access on every listed path. Final copyright,
attribution, and distribution terms for extracted source are decided during the
destination-repository migration and block publication, not private internal
implementation.

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

- Enable GitHub private vulnerability reporting as part of the M6 change to
  public visibility.
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
6. before M6, the private pre-release contact path is reachable; during the M6
   visibility change, private vulnerability reporting is enabled and reachable;
7. Dependabot alerts and security updates are enabled.
