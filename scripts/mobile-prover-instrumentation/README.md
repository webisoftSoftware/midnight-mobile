# Mobile prover instrumentation

Measurement rig for on-device proving. Nothing here ships: the patches are
applied to a staged copy of the pinned prover crates, and to the runtime crate
in the working tree, and both are reverted before any artifact is built.

This exists because the Phase A instrumentation that produced every per-stage
proving figure in `docs/performance/mobile/` was never committed. It lived in a
fork outside the repository plus a handful of in-repo edits that were reverted,
so the numbers in `gpu-prover-feasibility-2026-08-05.md` — Galaxy S10, k=15
zswap spend, MSM 51.8%, FFT 18.1%, remainder 30.1% — could not be reproduced
without rebuilding the rig from scratch. Keeping the edits as patch files is
what let the extension run dozens of A/Bs against a stable baseline; see
`packages/wallet-runtime-browser/wasm-prover-fork/` in the wallet repository,
which these files are modelled on.

## What the patches contain

| Patch                    | Applies to                                     | Adds                                                                                                             |
| ------------------------ | ---------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `phase-counters.patch`   | staged `midnight-curves` and `midnight-proofs` | `oneam_profile`, the counter module; MSM and FFT counters; thirteen counters splitting the PLONK prover's phases |
| `runtime-snapshot.patch` | `crates/runtime` in the working tree           | drains a snapshot per proof and forwards it in the `take_timings` JSON as `phaseCounters`                        |

`phase-counters.patch` counts MSM at `msm_specific`, not at `msm_best`: aarch64
takes the blstrs `multi_exp` fast path and never reaches `msm_best`, so a
counter there reads zero on the device that matters.

The counters are process-wide, so a snapshot is only attributable with admission
pinned to 1 — `setMaxConcurrency(1)` through the local prover, which is the same
reason that knob was exposed to JS.

## The loop

Stage, measure, revert. Each step is reversible and the last one leaves the tree
exactly as it was.

```sh
node scripts/mobile-prover-instrumentation/stage.mjs stage
git apply scripts/mobile-prover-instrumentation/runtime-snapshot.patch
```

Then build and run against the device probe in `tools/android-prover-spike`,
which drives the native prover directly. `setProfiling(true)` turns the counters
on and `takeTimings()` drains them; each sample carries `phaseCounters` as a
JSON object of nanosecond totals.

Revert before building anything shipped:

```sh
git checkout -- crates/runtime
node scripts/mobile-prover-instrumentation/stage.mjs revert
```

`revert` removes `.prover-fork/`, removes the generated `.cargo/config.toml`,
and restores `Cargo.lock` from the copy `stage` saved — a path override drops
the registry source and checksum lines for the crates it replaces, so the
lockfile needs restoring, not just the source tree.

## Changing the patches

Edit the staged tree, then capture. Never hand-edit the patch files: they are
generated, and the pinned crate versions are read from `Cargo.lock` at stage
time, so a dependency bump makes `stage` fail on a rejected hunk rather than
silently instrumenting the wrong source.

```sh
ONEAM_STAGE_WITHOUT_PATCHES=1 node scripts/mobile-prover-instrumentation/stage.mjs stage
# edit .prover-fork/**
node scripts/mobile-prover-instrumentation/stage.mjs capture > \
  scripts/mobile-prover-instrumentation/phase-counters.patch
```

For the runtime side, apply the patch, edit `crates/runtime`, and regenerate
with `git diff -- crates/runtime > .../runtime-snapshot.patch`.

## Verified so far

- Both patches apply to the pinned sources, and the instrumented workspace
  compiles and passes `cargo test --lib`.
- The counters record: one k=12 FFT through `best_fft` reports a non-zero
  `fft_ns` with `fft_calls` of 1, and stops accumulating after `finish`.
- `revert` restores the tree, the lockfile, and a clean pinned build.

Not yet verified: the on-device numbers. Reproducing the S10 baseline and
recording the remainder split needs the phone.
