# Vendored third-party crates

This directory holds upstream Rust crates that the local prover needs in a
changed form. Each crate keeps its upstream layout, formatting, and license
files so that it can be compared with the upstream release. The repository's
formatting and source-policy checks skip the upstream sources for that reason.

Every local change is listed here. Keep this list accurate: it is the review
surface for code that the upstream projects did not publish.

## `midnight-proofs-0.7.3` and `midnight-proofs-0.8.3`

Source: the crates.io releases `midnight-proofs` 0.7.3 and 0.8.3 (MIT OR
Apache-2.0). The local prover links both: 0.7.3 proves Ledger 8 circuits such as
Zswap and Dust, and 0.8.3 proves ZKIR v3 circuits. `[patch.crates-io]` in the
workspace `Cargo.toml` replaces the registry crates with these copies.

Local changes, the same in both versions:

| File                  | Change                                                                                                                                                                                                     |
| --------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/gpu.rs`          | New. The accelerator hooks: an optional commitment MSM, FFT, and inverse FFT function per process, and a `warm` function that shows the accelerator a parameter set's base vectors before the first proof. |
| `src/lib.rs`          | Declares the `gpu` module.                                                                                                                                                                                 |
| `Cargo.toml`          | Allows the `mismatched_lifetime_syntaxes` lint, which is newer than the release and would warn on every build.                                                                                             |
| `src/poly/kzg/mod.rs` | `commit` and `commit_lagrange` offer the MSM to the hook before they call `msm_specific`.                                                                                                                  |
| `src/poly/domain.rs`  | The domain's FFT and inverse FFT offer the transform to the hook before they call `best_fft`.                                                                                                              |

The hooks follow the design of the GPU prover proposed in
[midnightntwrk/midnight-zk#667](https://github.com/midnightntwrk/midnight-zk/pull/667),
which targets `midnight-proofs` 0.9. That pull request does not cover 0.7 or
0.8, so this is an independent backport of its hook contract:

- A hook returns the exact value of the stock function, or it declines and the
  stock function runs on the unchanged input.
- A hook draws no random value and touches no transcript.
- With no hook installed, the crates run only the code of the upstream releases.

To see the exact changes, compare a directory with the registry source:

```sh
cargo fetch
diff -ru ~/.cargo/registry/src/*/midnight-proofs-0.7.3/src \
  third_party/midnight-proofs-0.7.3/src
```

## `midnight-metal`

Source: the `metal` crate of
[SeungheonOh/midnight-zk](https://github.com/SeungheonOh/midnight-zk), branch
`metal-prover`, commit `3531dff44cc66c5c6afaa15d49bae3b207362b04` (Apache-2.0).
It is a Metal port of the CUDA backend of midnightntwrk/midnight-zk#667: Metal
kernels for the BLS12-381 G1 MSM and the scalar-field NTT, with the device
context, buffer zeroing, and admission logic around them.

Upstream builds for macOS only. Local changes:

| File                     | Change                                                                                                                                                                                              |
| ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Cargo.toml`             | Stands alone instead of inheriting from the upstream workspace. Depends on `midnight-curves` 0.3.1 from crates.io. Enables the Apple dependencies on iOS as well as macOS.                          |
| `build.rs`               | Compiles the shaders for the SDK of the build target: macOS, iOS device, or iOS simulator (iOS 18.0 or later). Compiles from the shader directory with relative names so no build path is embedded. |
| `Cargo.toml`, `build.rs` | A `quotient` feature, off by default, gates the quotient shader file.                                                                                                                               |
| `src/**/*.rs`            | Every `target_os = "macos"` gate also accepts `target_os = "ios"`.                                                                                                                                  |
| `src/ctx.rs`             | On iOS, the pool of idle GPU buffers keeps at most 256 MiB by default. Upstream keeps a quarter of the device's recommended working set, which is more than an iOS app may use.                     |
| `src/msm/split.rs`       | The CPU share of an MSM calls blst's Pippenger directly, because `midnight-curves` 0.3.1 has no `msm_pippenger`. The canonical copy of the scalars it makes is zeroed after use.                    |
| `src/tests.rs`           | Uses a local copy of the limb adapters instead of the `midnight-proofs` 0.9 test support. The part of one test that proved a circuit through the 0.9 quotient hook is removed.                      |
| `tests/` (directory)     | Removed. The upstream integration tests need `midnight-proofs` 0.9. `crates/prover-accel/tests` covers the same kernels through the two prover versions that this repository links.                 |

The quotient kernels are left out of the compiled shader library unless the
crate's `quotient` feature is on: the prover versions in this repository have no
quotient hook, so nothing calls them, and a quotient call declines.

## How the accelerator is guarded

The accelerator is off until the host application turns it on. Every proof is
verified before it is returned, and a proof that fails after the GPU worked on
it is made again on the CPU. See [docs/gpu-proving.md](../docs/gpu-proving.md).

## Update a vendored crate

1. Replace the directory with the new upstream source.
2. Apply the changes listed above again.
3. Run `cargo test --package midnight-mobile-prover-accel --release` on a Mac
   with `MIDNIGHT_MOBILE_REQUIRE_GPU=1`. The tests compare every accelerated
   operation, and whole proofs, with the stock CPU prover.
4. Update this file.
