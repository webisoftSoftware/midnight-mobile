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

`stage` refreshes `Cargo.lock` offline after writing the override, because
`runtime-snapshot.patch` adds a direct `midnight-curves` edge and every
`--locked` build -- including the device probe's -- refuses to start otherwise.

Then build and run against the device probe in `tools/android-prover-spike`:

```sh
node tools/android-prover-spike/scripts/prepare-artifacts.mjs
GRADLE=/path/to/gradle-9.0.0/bin/gradle \
  node tools/android-prover-spike/scripts/build.mjs
node tools/android-prover-spike/scripts/run-device.mjs \
  profiling=true maxConcurrency=1
```

The probe takes measurement extras as `key=value` arguments: `profiling=true`
turns the native stage instrumentation on and logs a `PROBE_TIMINGS` line per
drain, `maxConcurrency=N` pins the admission limit, and `batch=true` proves a
spend plus two outputs through one `proveBatch` call, which is the shape a real
send emits. Each drained sample carries `phaseCounters` as a JSON object of
nanosecond totals.

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

## Measuring on the device

Two things will silently ruin a run.

**Wake the phone first.** A dozing S10 parks its performance cores, and proving
measured while it dozes is about **2.4x slower** than the same build measured
awake -- 31 s versus 13 s for the k=15 spend. Nothing in the output says the
device was throttled, and the inflated numbers look perfectly self-consistent.
Send `input keyevent KEYCODE_WAKEUP` before each run; the probe activity holds
`FLAG_KEEP_SCREEN_ON` once it starts, so only the install window is exposed.
Doze also wedges `adb install` of the 42 MB APK indefinitely.

**Pin admission to 1 for any per-proof number.** At the default limit of 2 the
same k=15 spend reports between 11.4 s and 20.5 s depending only on whether a
neighbour is crowding it, while the batch's wall clock _drops_ by about 20%. Use
`maxConcurrency=1` for attribution and batch wall clock for throughput; never
mix them.

## Verified so far

- Both patches apply to the pinned sources, and the instrumented workspace
  compiles and passes `cargo test --lib`.
- The counters record: one k=12 FFT through `best_fft` reports a non-zero
  `fft_ns` with `fft_calls` of 1, and stops accumulating after `finish`.
- `revert` restores the tree, the lockfile, and a clean pinned build.
- **Measured on the S10** (SM-G973W, Android 12, 2026-08-19). The rig reproduces
  the standing baseline to within about 1%: MSM 51.0% against a recorded 51.8%,
  FFT 18.6% against 18.1%, and a non-MSM/FFT remainder of 30.4% against 30.1%.
  The phase counters account for 99.1% of the prove call. See
  `docs/performance/mobile/remainder-decomposition-2026-08-19.md` in the wallet
  repository.

Instrumentation costs about 15% of the prove call (13.19 s against 11.46 s
uninstrumented, with three uninstrumented controls agreeing to within 3.6%), so
compare shares between staged runs rather than absolute times against an
unstaged one.
