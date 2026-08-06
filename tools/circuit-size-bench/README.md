# Circuit size bench

Measures what a circuit's `k` costs the local prover: prove time, peak memory,
and the point at which a device stops being able to do it at all. It exists
because the practical limit on local proving is circuit size rather than
artifact packaging, and because that limit is only credible when measured.

The instrument is a synthetic single-circuit contract whose size is set by a
hash-round count. Each round is a fixed number of constraint rows, so **doubling
the rounds adds one to `k`**, which gives an evenly spaced ladder with nothing
else changing between rungs:

| Rounds | 4   | 8   | 16  | 32  | 64  | 128 | 256 |
| ------ | --- | --- | --- | --- | --- | --- | --- |
| `k`    | 14  | 15  | 16  | 17  | 18  | 19  | 20  |

Recorded results, and the device failure at k=19, are in
[the circuit size ceiling](../../docs/LOCAL_PROVER_CIRCUIT_SIZE.md). This
document is how to reproduce them.

## Parts

- `scripts/gen-circuit.mjs` writes a `.compact` source for a given round count.
- `scripts/build-request.mjs` executes a compiled circuit and emits a real
  `/prove` request. It runs the circuit locally -- no network, no providers, no
  deployed contract -- because the IR machine rejects a malformed transcript
  during preprocessing, long before the polynomial work being measured.
- The `circuit-size-bench` example in `crates/runtime` performs the measurement,
  driving `midnight_mobile_local_prover_configure` and
  `midnight_mobile_local_prover_prove` through the same C ABI a platform bridge
  uses.

`build-request.mjs` imports `@midnight-ntwrk/compact-runtime` and
`@midnight-ntwrk/ledger-v8`, which are **not** dependencies of this repository.
They resolve from the consuming wallet checkout, so run it with that checkout's
modules reachable -- for example by linking them beside the script's working
directory.

## Reproducing the ladder on a host

Requires the `compact` toolchain for compiling, and the SRS parameters for each
`k` from `https://srs.midnight.network/bls_midnight_2p<k>` staged in one
directory.

```sh
node tools/circuit-size-bench/scripts/gen-circuit.mjs 16 0 /tmp/bench/src
compact compile /tmp/bench/src/BenchR16_S0.compact /tmp/bench/managed/R16_S0
node tools/circuit-size-bench/scripts/build-request.mjs /tmp/bench/managed/R16_S0 /tmp/bench/req.R16.bin
cargo build --release --example circuit-size-bench
```

Then measure one rung. Peak memory is the reason each `k` gets its own process,
so run it under a peak-memory reporter rather than in a loop inside one:

```sh
/usr/bin/time -l ./target/release/examples/circuit-size-bench prove /tmp/bench/params 16 /tmp/bench/req.R16.bin
```

On macOS read **`peak memory footprint`**, not `maximum resident set size`:
above k=18 the resident figure is clamped by host memory reclaim while footprint
is not, and footprint is also what an iOS per-application memory limit is
measured against.

Above roughly 32 MB of prover key -- k=18 and up -- an inlined request exceeds
the ABI's 64 MB request cap. Build those with `--no-inline-keys` and pass the
managed directory so the circuit is served from the registry instead:

```sh
node tools/circuit-size-bench/scripts/build-request.mjs /tmp/bench/managed/R64_S0 /tmp/bench/req.R64.bin --no-inline-keys
./target/release/examples/circuit-size-bench prove /tmp/bench/params 18 /tmp/bench/req.R64.bin /tmp/bench/managed/R64_S0
```

## Calibration

A synthetic rung is only meaningful if it measures the same work as a real
circuit of the same `k`. The `zswap` subcommand proves a packaged circuit from
the crate's pinned deterministic request, so its numbers can be compared against
the recorded device benchmark baseline:

```sh
./target/release/examples/circuit-size-bench zswap /tmp/bench/params /path/to/packaged/artifacts spend
```

The k=14 synthetic rung and the k=14 packaged Zswap output circuit have been
observed within about 10% of each other on both time and peak memory.

Two smaller subcommands support the sweep. `irk <file.bzkir>` prints the `k` an
IR requires, which is how each rung is labelled -- and is the value a caller
would need in order to refuse an oversized circuit before proving it.
`params <file> <k>` decodes one SRS parameter file through the registry's own
path, including the assertion that rejects a file whose degree does not match
its declared `k`.

## Running on a device

Build the example for the device architecture and run it as a shell binary with
the artifacts pushed alongside. Note what this does and does not measure: a
shell binary is not subject to any per-application memory policy, so a device
figure obtained this way is an upper bound, and an in-application ceiling sits
at or below it.

```sh
cargo build --release --target aarch64-linux-android --example circuit-size-bench
adb push target/aarch64-linux-android/release/examples/circuit-size-bench /data/local/tmp/kbench/
```

The linker and archiver for that target come from the NDK; set
`CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER`, `CC_aarch64_linux_android`, and
`AR_aarch64_linux_android` to the prebuilt toolchain. On device, `peak_rss_mb`
is reported in the output line, read from procfs.

**A rung above the device's capacity does not fail safely.** On the reference
Galaxy S10, k=19 panicked the kernel and hard-reset the phone, reproducibly --
memory pressure starves the software watchdog and the device reboots. It is not
an allocation error, not a process kill, and nothing catches it. So sweep upward
one rung at a time, expect to lose the device when you cross the line, and do
not automate an unattended climb past the last rung known to complete.
