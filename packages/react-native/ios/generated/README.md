# Generated Swift bindings

UniFFI generates Swift code, a C header, and a module map that call the Rust
runtime. Run `npm run generate:bindings` from the repository root to update this
directory.

`npm run check:bindings` generates the files twice and requires identical
output. It then compares that output with the files committed here.

This directory contains source bindings only. Native libraries are packaged
separately.
