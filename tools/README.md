# Repository tools

`tools/` contains focused developer utilities. Unlike `scripts/`, these are not
general quality or release entry points. Run the root `package.json` scripts for
normal repository work.

| Directory                        | Purpose                                                                  | Used by normal automation?                            |
| -------------------------------- | ------------------------------------------------------------------------ | ----------------------------------------------------- |
| `bindgen/`                       | UniFFI binding generator invoked by the binding check                    | Yes, through `check:bindings`                         |
| `android-prover-spike/`          | Builds and runs the real local prover on an Android device               | Its artifact staging is also used by the Expo example |
| `ios-local-prover/`              | Builds a temporary Expo consumer and validates the prover in a simulator | No                                                    |
| `circuit-size-bench/`            | Generates and measures synthetic circuits one size at a time             | No                                                    |
| `mobile-prover-instrumentation/` | Applies reversible profiling patches for device measurements             | No                                                    |

The Android and iOS harnesses write only under `target/` and generated native
build directories. The instrumentation rig is the exception: it temporarily
patches the working tree and must be reverted as described in its README.

## Local prover harnesses

Stage the pinned public proving artifacts before using either platform harness:

```sh
node tools/android-prover-spike/scripts/prepare-artifacts.mjs
```

For Android, set `ANDROID_HOME` or `ANDROID_SDK_ROOT`, provide Gradle 9 when it
is not on `PATH`, connect an arm64 device, then run:

```sh
GRADLE=/path/to/gradle node tools/android-prover-spike/scripts/build.mjs
node tools/android-prover-spike/scripts/run-device.mjs
```

For iOS, choose an installed simulator explicitly:

```sh
IOS_PROVER_SIMULATOR_UDID=<simulator-udid> \
  node tools/ios-local-prover/scripts/run-simulator.mjs
```

Use `xcrun simctl list devices` to find a simulator UDID.
