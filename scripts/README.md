# Repository scripts

The root `package.json` is the public entry point for repository automation.
Implementation files are grouped by responsibility:

- `native/`: Android and Apple builds, binary validation, and native consumer
  checks.
- `package/`: React Native package and generated UniFFI binding checks.
- `quality/`: shared command helpers, lint and test runners, coverage policy,
  and source policy.
- `release/`: release input validation, evidence generation, and artifact
  verification.
- `security/`: dependency, vulnerability, and secret-scanning policy.
- `wallet-core/`: public command parity, focused product-source policy, and
  shipped artifact validation.

Keep tests beside the module they cover using the `*.test.mjs` suffix. The
quality test runner discovers them recursively, so tests do not need individual
`package.json` entries.

Manual developer tools that are not part of package, quality, or release
automation belong under `tools/` rather than here.
