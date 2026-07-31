# iOS local prover validation

This focused Expo release application validates the separately imported
`@1am/midnight-mobile/local-prover` module on an arm64 iOS Simulator. It stages
the same ignored, SHA-256-pinned Zswap spend artifacts used by the Android
probe, copies them into the built application bundle, and performs Ledger 8.1.0
`/check` and `/prove` operations without Metro or runtime network access.

Run the harness against the default installed iPhone 17 simulator:

```sh
node tools/ios-local-prover/scripts/run-simulator.mjs
```

Override the destination with `IOS_PROVER_SIMULATOR_UDID`. The script builds the
three-slice local-prover XCFramework once, prebuilds a temporary Expo consumer,
ad-hoc signs and launches its release application, pulls the check response and
proof from the application container, and validates both with the host Rust
codec. All generated projects, proving blobs, application bundles, and reports
remain under ignored `target/` or `artifacts/` paths.

The simulator validates Apple packaging, Expo/Swift/C/Rust integration, mapped
bundle-file handling, and response compatibility. Its host-backed memory and CPU
behavior do not establish physical-iPhone resource feasibility.
