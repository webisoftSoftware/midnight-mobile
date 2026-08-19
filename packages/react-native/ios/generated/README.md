# Generated Swift bindings

The `npm run generate:bindings` command generates the UniFFI Swift source, C
header, and module map from the Rust runtime. It writes these files to this
directory.

The `npm run check:bindings` command generates these files twice. It checks that
both outputs match byte-for-byte. It then compares the generated files with the
reviewed files byte-for-byte.

This directory does not contain native libraries.
