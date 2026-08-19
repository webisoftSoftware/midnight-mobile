# Generated Kotlin bindings

The `npm run generate:bindings` command generates the UniFFI Kotlin source from
the Rust runtime. It writes the source to this directory.

The `npm run check:bindings` command generates the Kotlin source twice. It
checks that both outputs match byte-for-byte. It then compares the generated
source with the reviewed source byte-for-byte.

Consumer builds use the packaged source. They do not run Cargo.
