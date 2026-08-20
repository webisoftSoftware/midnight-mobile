# Generated Kotlin bindings

UniFFI generates Kotlin code that calls the Rust runtime. Run
`npm run generate:bindings` from the repository root to update this directory.

`npm run check:bindings` generates the code twice and requires identical output.
It then compares that output with the files committed here.

Applications compile the packaged Kotlin files. They do not run Cargo or
generate bindings during a build.
