# Repository tools

`tools/` contains utilities for focused development and measurement tasks. For
normal builds, tests, and releases, use the npm commands in the root
`package.json`.

| Directory                        | Purpose                                                                 |
| -------------------------------- | ----------------------------------------------------------------------- |
| `bindgen/`                       | Generates UniFFI bindings for `check:bindings`                          |
| `android-prover-spike/`          | Builds and tests the local prover on an Android device                  |
| `ios-local-prover/`              | Builds a temporary Expo app and tests the prover in an iOS simulator    |
| `circuit-size-bench/`            | Generates and measures one synthetic circuit size per process           |
| `mobile-prover-instrumentation/` | Applies temporary profiling patches for measurements on a mobile device |

The Android and iOS tools write under `target/` and generated native build
directories. The instrumentation tool is different: it changes the working tree
temporarily. Follow its README and restore the tree after every run.

## Local prover harnesses

Prepare and verify the pinned public prover files before using either platform
tool:

```sh
node tools/android-prover-spike/scripts/prepare-artifacts.mjs
```

For Android:

1. Set `ANDROID_HOME` or `ANDROID_SDK_ROOT`.
2. Install Gradle 9 or set `GRADLE` to its executable.
3. Connect an arm64 Android device.
4. Run:

```sh
GRADLE=/path/to/gradle node tools/android-prover-spike/scripts/build.mjs
node tools/android-prover-spike/scripts/run-device.mjs
```

For iOS, choose an installed simulator:

```sh
IOS_PROVER_SIMULATOR_UDID=<simulator-udid> \
  node tools/ios-local-prover/scripts/run-simulator.mjs
```

Use `xcrun simctl list devices` to find a simulator UDID.
