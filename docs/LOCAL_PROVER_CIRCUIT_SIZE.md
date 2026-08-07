# Local prover circuit size ceiling

Measured 2026-08-06 on an M4 Pro host, driving the real local-prover C ABI
(`configure` then `prove`) with the packaged Ledger 8.1.0 artifacts and the
crate's pinned deterministic Zswap requests, one circuit per process so peak
resident size attributes to a single `k`. Reproduce any figure here with
[`tools/circuit-size-bench`](../tools/circuit-size-bench/README.md).

## Proving cost doubles per `k` step

| Circuit | `k` | Prove | Peak RSS |
| --- | --- | --- | --- |
| `midnight/zswap/output` | 14 | 651-700 ms | 273-301 MB |
| `midnight/zswap/spend` | 15 | 1248-1338 ms | 580-588 MB |

The ratio is 2.09x across three runs, matching the `O(2^k)` expectation. The
measured k=15 prove time agrees with the host baseline recorded by the device
benchmark, so the two measure the same work.

## Measured ladder, k=14 through k=20

Confirmed by sweeping synthetic single-circuit contracts whose `k` is set by a
hash-round count -- one `k` step per doubling of rounds, 4 rounds giving k=14 and
256 giving k=20. Each was executed locally with `compact-runtime`, its proof data
serialized into a real preimage, and proved through the same C ABI. Circuits above
roughly 32 MB of prover key are served from the registry because an inlined
request exceeds the `MAX_REQUEST_BYTES` cap of 64 MB.

| `k` | Rounds | Prover key | Host prove | Host peak footprint | Device prove | Device peak RSS |
| --- | --- | --- | --- | --- | --- | --- |
| 14 | 4 | 5 MB | 0.9 s | 248 MB | 7.7 s | 231 MB |
| 15 | 8 | 9.5 MB | 1.6 s | 533 MB | 14.0 s | 435 MB |
| 16 | 16 | 18.6 MB | 3.5 s | 1053 MB | 24.0 s | 860 MB |
| 17 | 32 | 36.8 MB | 6.9 s | 2342 MB | 44.3 s | 1699 MB |
| 18 | 64 | 73.1 MB | 9.8 s | 4555 MB | 81.3 s | 3405 MB |
| 19 | 128 | 145.8 MB | 21.4 s | 7946 MB | **device kernel panic** | -- |
| 20 | 256 | 291.1 MB | 354.8 s | 13794 MB | not attempted | -- |

Device figures are a Galaxy S10 (`SM-G973W`, 7.6 GB RAM, about 4.5 GB available),
running the probe as a shell binary so no per-application memory policy applies --
an in-application ceiling is at or below these numbers, never above. Device prove
time is a consistent 8-9x the host. Host figures are an M4 Pro; `peak memory
footprint` is quoted rather than maximum resident size because above k=18 the
resident figure is clamped by host memory pressure while footprint is not.

## Where it stops

k=18 completes on the reference device at 3.4 GB and 81 s. **k=19 panics the
kernel and hard-resets the phone**, reproducibly, twice out of two attempts:

```text
Kernel panic - not syncing: Software Watchdog Timer expired 100s
sec_debug_panic_handler :Software Watchdog Timer expired 100s
(sec_debug_pm_restart) rebooting...
```

The failure is not a graceful allocation error and not a process kill. Memory
pressure starves the kernel watchdog thread for 100 s and the device reboots, so
a caller gets no error to handle and the user loses the whole device. **A circuit
that is too large must therefore be refused before proving starts, not attempted
and recovered from.**

## Parameters are not the constraint

`srs.midnight.network` publishes `bls_midnight_2p10` through `bls_midnight_2p25`.
Every file through k=18 decodes through `ParamsProver::read` and satisfies the
registry's `max_k` assertion. Decoding costs about 1.5x the file size resident
and about 2 ms/MB: the full k=10 through k=18 set is 96 MB of files, 146 MB
decoded, 173 ms to decode. Packaging or downloading larger parameters is
therefore cheap, and it does not raise the ceiling above -- a larger parameter
set only lets a proof that cannot fit in memory begin.

## Every shipped circuit is at or below k=15

Read from the `.bzkir` sources: Zswap spend 15, Zswap output 14, Zswap sign 13,
Dust spend 13. The k=10 through k=15 parameter set packaged by the application is
already complete for them.

## Consequence for larger circuits

A circuit above k=15 should be routed to a remote proof server rather than proved
locally: k=16 already costs 24 s and 860 MB on the reference device, and k=19
takes the device down outright.

Today nothing refuses such a circuit early. A missing parameter surfaces late, as
a `get_params` lookup failure inside `run_prove` reported as the generic
`PROOF_FAILED`, which a caller cannot distinguish from a genuine proof failure --
and when the parameter is present, proving simply proceeds until the device dies.
Exposing the `k` an IR requires, and refusing a circuit above a supported bound
before any proving begins, is what makes a remote fallback possible at all.
