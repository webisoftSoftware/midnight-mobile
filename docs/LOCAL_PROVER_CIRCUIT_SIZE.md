# Local prover circuit size ceiling

Measured 2026-08-06 on an M4 Pro host, driving the real local-prover C ABI
(`configure` then `prove`) with the packaged Ledger 8.1.0 artifacts and the
crate's pinned deterministic Zswap requests, one circuit per process so peak
resident size attributes to a single `k`.

## Proving cost doubles per `k` step

| Circuit | `k` | Prove | Peak RSS |
| --- | --- | --- | --- |
| `midnight/zswap/output` | 14 | 651-700 ms | 273-301 MB |
| `midnight/zswap/spend` | 15 | 1248-1338 ms | 580-588 MB |

The ratio is 2.09x across three runs, matching the `O(2^k)` expectation. The
measured k=15 prove time agrees with the host baseline recorded by the device
benchmark, so the two measure the same work.

Extrapolating: k=16 is roughly 1.2 GB peak, k=17 roughly 2.5 GB, k=18 roughly
5.2 GB. A k=16 proof alone exceeds the iOS per-application memory budget, and
prove time scales with it -- roughly 23 s at k=16 on the reference Android
device against 11.5 s at k=15. **On-device proving is bounded at k=15, and the
bound is proving memory rather than artifact packaging.**

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

A circuit above k=15 should be routed to a remote proof server rather than
proved locally. Today a missing parameter surfaces late, as a `get_params`
lookup failure inside `run_prove` reported as the generic `PROOF_FAILED`, which a
caller cannot distinguish from a genuine proof failure. Exposing the `k` an IR
requires, and a distinct error for a circuit the registry cannot serve, would let
a caller fall back before starting work it cannot finish.
