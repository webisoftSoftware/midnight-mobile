# Repository scripts

Run repository automation through the scripts in the root `package.json`. The
files in this directory implement those commands and are grouped by purpose:

- `native/`: Android and Apple builds, binary validation, and native consumer
  checks.
- `package/`: React Native package and generated UniFFI binding checks.
- `quality/`: shared helpers, lint and test runners, coverage rules, and source
  rules.
- `release/`: release input validation, evidence generation, and artifact
  verification.
- `security/`: dependency, vulnerability, and secret-scanning policy.
- `wallet-core/`: public command consistency, product-source rules, and shipped
  artifact validation.

Name tests `*.test.mjs` and keep them beside the module they test. The test
runner finds them recursively; do not add a separate `package.json` command for
each test.

Put manual developer utilities under `tools/`, not `scripts/`.
