# Circuit size benchmark

This harness measures the local prover at one circuit size (`k`) per process.
Keeping each run isolated makes peak memory and elapsed time attributable to a
single parameter set.

The Rust entry point is the `circuit-size-bench` example in the runtime crate.
It calls the shipping C ABI rather than reimplementing prover setup.

## Inputs

- `scripts/gen-circuit.mjs` writes a synthetic Compact contract. Hash rounds
  control circuit size; ledger slots independently control public-input count.
- `scripts/build-request.mjs` turns a compiled managed contract directory into a
  real prove request. It intentionally resolves Compact and Ledger packages from
  the consuming wallet checkout, because those packages are not repository
  dependencies.
- `tools/android-prover-spike/scripts/prepare-artifacts.mjs` stages the pinned
  Zswap calibration artifacts and parameters under
  `target/android-prover-spike/artifacts/`.

## Commands

Build the Rust example once:

```sh
cargo build --release --package midnight-mobile-runtime \
  --example circuit-size-bench
```

Inspect a parameter file or compiled IR:

```sh
cargo run --release --package midnight-mobile-runtime \
  --example circuit-size-bench -- params <parameter-file> <k>
cargo run --release --package midnight-mobile-runtime \
  --example circuit-size-bench -- irk <file.bzkir>
```

Run the packaged Zswap calibration or a prepared synthetic request:

```sh
cargo run --release --package midnight-mobile-runtime \
  --example circuit-size-bench -- \
  zswap <parameter-directory> <artifact-directory> spend

cargo run --release --package midnight-mobile-runtime \
  --example circuit-size-bench -- \
  prove <parameter-directory> <k> <request-file> [managed-directory]
```

Use `--no-inline-keys` when building requests whose proving key would exceed the
runtime request-size limit; pass the corresponding managed directory to the
`prove` command so the registry can load those keys.
