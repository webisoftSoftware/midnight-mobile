# Generated Kotlin binding location

`npm run generate:bindings` regenerates the UniFFI Kotlin source here from the
sanitized runtime ABI. `npm run check:bindings` repeats generation twice and
rejects drift before comparing the reviewed file byte-for-byte. Consumer builds
use the packaged source and never invoke Cargo.
