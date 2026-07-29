# Midnight Mobile

An experimental, community-maintained React Native SDK for the Midnight wallet
runtime.

<!-- prettier-ignore -->
> [!IMPORTANT]
> This project is maintained by 1AM contributors. It is not an official
> Midnight SDK and is not affiliated with or endorsed by the Midnight
> Foundation.

The first planned release is `@1am/midnight-mobile@0.1.0-alpha.1`. It will
contain wallet-core functionality only. Social features, FT/NFT support,
contract verifier artifacts, 1AM gateway authentication, and private fast sync
are explicitly out of scope.

Repository controls and quality tooling are established; runtime extraction is
the current workstream. See [PLAN.md](./PLAN.md) for the accepted extraction,
security, packaging, and release plan.

For the milestone-by-milestone delivery roadmap, see
[docs/LONG_TERM_PROJECT_PLAN.md](./docs/LONG_TERM_PROJECT_PLAN.md).

## Status

- Repository bootstrap: complete
- Sanitized source extraction: ready for a fresh source-state bracket
- Public release: not started

## License

Repository-authored material is available under `MIT OR Apache-2.0`; see
[LICENSE-MIT](./LICENSE-MIT) and [LICENSE-APACHE](./LICENSE-APACHE). Importing
source-derived material remains subject to the dependency, provenance, and
sanitization audit described in the plan and [NOTICE](./NOTICE). Final
copyright, attribution, and distribution terms for the extracted implementation
are deferred to the destination-repository migration and must be resolved before
publication.
