# GPU proving on iOS with Metal

Midnight Mobile proves Midnight zero-knowledge transactions on the phone. This
document describes the GPU accelerator of that local prover: what it does, how
to turn it on, how it is built, and how it was verified.

The accelerator is an implementation of Charles Hoskinson's GPU prover for
`midnight-zk`,
[midnightntwrk/midnight-zk#667](https://github.com/midnightntwrk/midnight-zk/pull/667),
inside the Mobile SDK. That work defines the design this accelerator follows:
optional hooks in the prover that either return the exact value of the CPU code
or decline, a device backend behind them, and the rules that keep proofs
identical and witness data contained. The Metal backend is the port of that
design by Seungheon Oh, from the `metal-prover` branch of
[SeungheonOh/midnight-zk](https://github.com/SeungheonOh/midnight-zk).

With it, a wallet proves shielded transfers and contract calls on the device,
with the prover's heaviest arithmetic running on the iPhone's GPU. In our first
measurements the proving call of a shielded spend takes about 30% less time than
on the CPU. This is the initial result: more of the prover moves to the GPU as
we optimize the accelerator and finalize this release for production.

## What it does

A PLONK proof over BLS12-381 spends most of its time in two operations:
multi-scalar multiplications (MSMs), which make the polynomial commitments, and
fast Fourier transforms (FFTs). With the accelerator on, the prover offers both
to the GPU:

| Operation                 | Without the accelerator | With the accelerator      |
| ------------------------- | ----------------------- | ------------------------- |
| Commitment MSM            | CPU (blst Pippenger)    | Metal, from 2^13 terms    |
| FFT and inverse FFT       | CPU                     | Metal, from 2^15 elements |
| Everything else           | CPU                     | CPU                       |
| Operations below the size | CPU                     | CPU                       |

Both prover versions that the SDK links are covered: `midnight-proofs` 0.7.3,
which proves Ledger 8 circuits such as Zswap spends and outputs, and
`midnight-proofs` 0.8.3, which proves ZKIR v3 contract circuits.

## Turn it on

```ts
import { createMidnightLocalProver } from "@1am/midnight-mobile/local-prover";

const prover = await createMidnightLocalProver(configuration);
await prover.setAccelerator?.(true);

const status = await prover.getAcceleratorStatus?.();
// {
//   availability: "active",
//   active: true,
//   device: "<name of the GPU>",
//   gpuOperations: 144,
//   cpuOperations: 20,
//   ...
// }
```

The switch applies to the whole process and stays set across prover registries.
`getAcceleratorStatus` reports whether the GPU is in use, the name of the GPU,
and how many prover operations ran on it.

| Platform       | Behaviour                                                                          |
| -------------- | ---------------------------------------------------------------------------------- |
| iOS 18+        | Uses the GPU through Metal                                                         |
| iOS 15.1 to 17 | The status reports `no-device`; proofs run on the CPU as before                    |
| Android        | The switch is accepted and the status reports `unsupported`; proofs run on the CPU |

The package's minimum iOS version does not change. An application that never
calls `setAccelerator` behaves exactly as before.

## How it is built

```text
 JavaScript   prover.setAccelerator(true) / prover.prove(request)
     │
 Swift        MidnightMobileLocalProverModule
     │
 Rust         crates/runtime          local prover, switch, status, CPU retry
     │
              crates/prover-accel     installs one accelerator into both provers,
     │                                keeps the GPU's copy of the parameters
     │
              third_party/midnight-proofs-0.7.3 and -0.8.3
     │                                commitment MSM, FFT, inverse FFT hooks
     │
              third_party/midnight-metal
     │                                Metal kernels, device context, zeroing
 GPU          Metal compute shaders, compiled into the framework
```

- **Hooks.** The two prover versions each gain a small `gpu` module with the
  hook contract of midnight-zk#667: a hook returns the exact value that the CPU
  code would return, or it declines and the CPU code runs on the unchanged
  input. A hook draws no randomness and touches no transcript.
- **Metal backend.** The Metal crate is built for iOS devices and both iOS
  simulators as well as macOS. Its shaders are compiled at build time for the
  SDK of each target and embedded in the framework.
- **Parameter tables.** The prover keeps its proving parameters as Jacobian
  points and may load them again for every proof. The accelerator converts each
  base vector once, keeps it on the GPU, and finds it again by content. The
  tables are prepared when the prover is configured, so the first proof does not
  pay for them.
- **Native interface.** Three C functions join the local prover's interface:
  `midnight_mobile_local_prover_set_accelerator`,
  `midnight_mobile_local_prover_accelerator_status`, and
  `midnight_mobile_local_prover_trim_memory`.

[third_party/README.md](../third_party/README.md) lists the upstream sources and
every change made to them.

## Correctness and safety

- **Same proofs.** Every accelerated operation returns the exact value of the
  CPU code. A proof made with the GPU is byte for byte the proof that the CPU
  prover makes from the same randomness.
- **Verified before it is returned.** The prover verifies every proof against
  the circuit's verifier key before it returns the proof.
- **CPU retry.** If a proof fails after the GPU worked on it, the prover makes
  the proof again on the CPU. If that succeeds, the accelerator turns itself off
  and the status reports `tripped`.
- **Declines fall back.** A GPU error, a failed allocation, or an operation that
  does not fit the device is a decline, and the CPU code runs. The caller never
  sees it.
- **Witness hygiene.** GPU buffers that held witness values are zeroed when a
  proof ends.
- **Memory.** The native modules release the accelerator's GPU memory when the
  operating system reports memory pressure, and the idle buffer pool is capped
  on iOS.

## Verification

The accelerator is covered at four levels. Each level compares the GPU with the
CPU prover in the same process.

| Level         | What it checks                                                                                            |
| ------------- | --------------------------------------------------------------------------------------------------------- |
| Metal kernels | Field arithmetic, MSM and FFT kernels against blst and the CPU reference                                  |
| Prover hooks  | Commitments, transforms and whole proofs for both prover versions; proofs are byte-identical to the CPU's |
| Local prover  | Real Zswap spend and output proofs with the official keys, a batch of five, the CPU retry, memory release |
| Application   | A React Native app proves on the CPU and on the GPU through JavaScript, Swift, Rust and Metal             |

The Rust suites run on macOS and inside the iOS simulator, where the tests
confirm that the GPU served the operations. The application harness runs in the
iOS simulator and validates both proofs on the host.

```sh
# The accelerator against the CPU prover, for both prover versions.
MIDNIGHT_MOBILE_REQUIRE_GPU=1 \
  cargo test --release --package midnight-mobile-prover-accel

# The application harness: builds the framework and a test app, proves on the
# CPU and on the GPU, and validates both proofs.
IOS_PROVER_SIMULATOR_UDID=<simulator> IOS_PROVER_REQUIRE_GPU=1 \
  node tools/ios-local-prover/scripts/run-simulator.mjs

# The same application on a connected iPhone.
IOS_PROVER_DEVICE_ID=<device> IOS_PROVER_DEVELOPMENT_TEAM=<team> \
  node tools/ios-local-prover/scripts/run-device.mjs
```

## Measurements

The first measurements, on Apple silicon (an M4 Max with the prover's four
threads), prove the Zswap circuits with the official keys:

| Proof                        | CPU    | GPU    | Time saved |
| ---------------------------- | ------ | ------ | ---------- |
| Spend (k = 15), proving call | 1.60 s | 1.14 s | 29%        |
| Output (k = 14)              | 1.1 s  | 0.85 s | 23%        |
| Spend, in the simulator app  | 1.52 s | 1.27 s | 16%        |

The GPU takes over 42% of the prover's CPU work on a spend proof. The same
change also stops every proof from re-deriving its circuit size, which saves
about a third of a second per spend proof on the CPU path as well.

This is where the accelerator starts. The commitments and the transforms run on
the GPU today. The next step follows midnight-zk#667 further and moves the
quotient polynomial to the GPU as well, which is most of the work that remains
on the CPU.

`tools/circuit-size-bench` and the example application in `examples/expo` report
the CPU and GPU durations on any machine or device.

## Credits

This accelerator exists because of Charles Hoskinson's GPU prover for Midnight.
The design, the hook contract, and the safety rules are his work in
[midnightntwrk/midnight-zk#667](https://github.com/midnightntwrk/midnight-zk/pull/667).

- **GPU prover design:** Charles Hoskinson,
  [midnightntwrk/midnight-zk#667](https://github.com/midnightntwrk/midnight-zk/pull/667).
- **Metal port:** Seungheon Oh,
  [SeungheonOh/midnight-zk](https://github.com/SeungheonOh/midnight-zk), branch
  `metal-prover`.
- **Idea and direction for the Mobile SDK:** Utkarsh Varma.
- **Mobile SDK integration:** Utkarsh Varma with Claude, the AI model from
  Anthropic.
