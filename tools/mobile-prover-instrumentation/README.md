# Mobile prover instrumentation

This tool measures proving work on a mobile device. It applies temporary
profiling patches to:

- A staged copy of the pinned prover crates.
- `crates/runtime` in the current working tree.

Nothing in this directory ships in the mobile package. Always start from a clean
working tree and restore it after a measurement.

## Why this tool exists

Earlier performance measurements used changes that were kept outside this
repository. Those measurements could not be reproduced without rebuilding the
profiling setup. This directory stores the changes as patch files so every run
can use the same baseline.

The original Galaxy S10 measurement used a Zswap spend circuit at `k=15`:

- Multi-scalar multiplication (MSM): 51.8%
- Fast Fourier transform (FFT): 18.1%
- All other work: 30.1%

## Patches

| Patch                    | Target                                         | Purpose                                                              |
| ------------------------ | ---------------------------------------------- | -------------------------------------------------------------------- |
| `phase-counters.patch`   | Staged `midnight-curves` and `midnight-proofs` | Adds MSM, FFT, and thirteen PLONK prover phase counters              |
| `runtime-snapshot.patch` | `crates/runtime`                               | Adds `phaseCounters` to each proof sample returned by `take_timings` |

`phase-counters.patch` records MSM calls at `msm_specific`. On arm64, the
`blstrs` implementation uses its `multi_exp` fast path and does not call
`msm_best`. A counter at `msm_best` would therefore report zero on the target
device.

The counters are shared by the entire process. Set the prover admission limit to
one with `maxConcurrency=1` when measuring a single proof. Otherwise, concurrent
proofs can contribute to the same sample.

## Run a measurement

### 1. Confirm that the tree is clean

Commit or stash unrelated work before applying the patches:

```sh
git status --short
```

### 2. Stage and apply the patches

```sh
node tools/mobile-prover-instrumentation/stage.mjs stage
git apply tools/mobile-prover-instrumentation/runtime-snapshot.patch
```

The `stage` command creates `.prover-fork/`, writes a Cargo path override, and
refreshes `Cargo.lock` offline. The lockfile update is required because the
runtime patch adds a direct `midnight-curves` dependency and device builds use
`--locked`.

### 3. Build and run the Android probe

```sh
node tools/android-prover-spike/scripts/prepare-artifacts.mjs
GRADLE=/path/to/gradle-9.0.0/bin/gradle \
  node tools/android-prover-spike/scripts/build.mjs
node tools/android-prover-spike/scripts/run-device.mjs \
  profiling=true maxConcurrency=1
```

The device probe accepts these `key=value` options:

- `profiling=true`: Enables phase counters and logs one `PROBE_TIMINGS` record
  for each sample.
- `maxConcurrency=N`: Sets the number of proof operations that may run at once.
  Use `1` for per-proof measurements.
- `batch=true`: Runs one spend and two outputs through one `proveBatch` call,
  which matches the shape of a normal send.

Each `PROBE_TIMINGS` record contains `phaseCounters`, a JSON object whose values
are measured in nanoseconds.

### 4. Restore the repository

Restore the runtime files, then let the staging tool remove its temporary files
and restore the saved lockfile:

```sh
git restore --source=HEAD -- crates/runtime
node tools/mobile-prover-instrumentation/stage.mjs revert
git status --short
```

The final status must match the status from step 1. The `revert` command removes
`.prover-fork/` and the generated `.cargo/config.toml`. It restores `Cargo.lock`
from the saved copy because removing a Cargo path override alone does not
restore registry source and checksum entries.

## Update a patch

Do not edit generated patch files by hand. The staging tool reads the pinned
crate versions from `Cargo.lock` and fails if a patch no longer applies.

To update the prover patch:

```sh
ONEAM_STAGE_WITHOUT_PATCHES=1 \
  node tools/mobile-prover-instrumentation/stage.mjs stage
# Edit files under .prover-fork/.
node tools/mobile-prover-instrumentation/stage.mjs capture > \
  tools/mobile-prover-instrumentation/phase-counters.patch
node tools/mobile-prover-instrumentation/stage.mjs revert
```

To update the runtime patch, apply it, edit `crates/runtime`, and capture the
result:

```sh
git diff -- crates/runtime > \
  tools/mobile-prover-instrumentation/runtime-snapshot.patch
```

## Get reliable device measurements

### Keep the device awake

A sleeping Galaxy S10 parks its performance cores. In testing, the same `k=15`
spend took about 31 seconds while the phone slept and 13 seconds while it was
awake. The output does not identify this throttling.

Wake the device before each run:

```sh
adb shell input keyevent KEYCODE_WAKEUP
```

The probe keeps the screen on after it starts. Waking the device also prevents
`adb install` from hanging while it installs the 42 MB application package.

### Use one proof at a time for attribution

At the default concurrency of two, the measured `k=15` spend ranged from 11.4 to
20.5 seconds depending on competing work. Use `maxConcurrency=1` when
attributing time to one proof. Use batch wall-clock time when measuring
throughput. Do not compare those two kinds of measurement directly.

## Verified results

- Both patches apply to the pinned sources.
- The instrumented workspace compiles and passes `cargo test --lib`.
- A `k=12` FFT reports a non-zero `fft_ns`, reports one `fft_calls`, and stops
  accumulating after `finish`.
- The revert process restores the sources, Cargo configuration, and lockfile.
- On a Galaxy S10 (SM-G973W, Android 12, 2026-08-19), the tool reproduced the
  earlier baseline within about 1%: MSM 51.0%, FFT 18.6%, and other work 30.4%.
  The phase counters accounted for 99.1% of the proof call.

Instrumentation added about 15% overhead in this test: 13.19 seconds with
instrumentation and 11.46 seconds without it. Compare phase percentages between
instrumented runs. Do not compare absolute times from instrumented and normal
builds.
