# UniFFI binding reproducibility

The React Native package commits reviewed Swift and Kotlin source generated from
the sanitized Rust runtime. It does not commit a native library at M1.

Run `npm run generate:bindings` after an intentional Rust ABI or UniFFI version
change. The command:

1. builds `midnight-mobile-runtime` using the locked Cargo graph;
2. generates Swift and Kotlin twice in separate temporary directories with
   optional formatter discovery disabled;
3. requires both output trees to be byte-for-byte identical;
4. verifies that the C, Swift, and Kotlin surfaces contain exactly these eight
   functions:
   - `open_wallet_session`;
   - `apply_sync_batch`;
   - `get_wallet_snapshot`;
   - `export_wallet_checkpoint`;
   - `begin_command`;
   - `resume_operation`;
   - `cancel_operation`;
   - `close_wallet_session`;
5. writes only generated source, header, and module-map files into the package.

`npm run check:bindings` performs the same clean generation and ABI checks but
does not modify the repository. It then compares every generated file with the
reviewed package copy and fails on missing, extra, or stale output. The root
quality gate runs this check.
