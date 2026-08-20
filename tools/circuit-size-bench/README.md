# Circuit size benchmark

This tool measures one local-prover circuit size, called `k`, in each process.
Running one size at a time makes its peak memory and elapsed time easier to
measure.

The Rust entry point is the `circuit-size-bench` example in the runtime crate.
It calls the same C application binary interface (ABI) that ships in the mobile
package.

## Inputs

- `scripts/gen-circuit.mjs` creates a synthetic Compact contract. Hash rounds
  control circuit size. Ledger slots control the number of public inputs.
- `scripts/build-request.mjs` turns a compiled managed contract directory into a
  real proof request. It loads Compact and Ledger packages from the wallet
  checkout because this repository does not depend on those packages.
- `tools/android-prover-spike/scripts/prepare-artifacts.mjs` stages the pinned
  Zswap calibration files and parameters under
  `target/android-prover-spike/artifacts/`.

## Commands

Build the Rust example once:

```sh
cargo build --release --package midnight-mobile-runtime \
  --example circuit-size-bench
```

Inspect a parameter file or compiled intermediate representation (IR):

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

Use `--no-inline-keys` when a proving key would exceed the runtime request-size
limit. Then pass its managed directory to the `prove` command so the runtime can
load the key from that directory.
