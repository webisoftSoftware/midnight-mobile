# Pull request

## Scope and user impact

Describe what changed, why it is needed, and the user-visible effect. Link the
issue and milestone exit criteria this pull request advances.

## Tests performed

List exact commands and relevant platforms. Explain any check that is not
applicable.

## Security and secret-handling impact

Describe changes to sensitive data, trust boundaries, endpoint configuration,
logging, storage, cancellation, or FFI handling. State `No security impact` only
after considering those areas.

## Generated artifacts

List generated declarations, bindings, native binaries, manifests, or snapshots
that changed. Include the reproduction command and review evidence, or state
`No generated artifact changes`.

## Review checklist

- [ ] The change stays within the accepted wallet-core milestone scope.
- [ ] No social, FT/NFT, verifier-artifact, gateway-authentication, private
      fast-sync, viewing-key-upload, or operated-service-default capability was
      introduced.
- [ ] No seed, checkpoint, credential, private endpoint, sensitive log,
      transaction payload, or non-synthetic fixture was committed.
- [ ] Success and relevant failure, malformed-input, cancellation, or regression
      cases are tested.
- [ ] `npm run quality` passes without warnings, skipped applicable checks, or
      undocumented suppressions.
- [ ] Public API, architecture, compatibility, and security documentation was
      updated where behavior changed.
- [ ] Copied or generated material has documented provenance and redistribution
      permission.
- [ ] Generated and packaged artifacts were inspected when applicable.
