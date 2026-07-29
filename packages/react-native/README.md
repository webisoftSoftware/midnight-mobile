# Midnight Mobile

This staging package exposes a strict React Native and Expo boundary for the
wallet runtime. Applications supply all service URLs, HTTP and WebSocket
implementations, credentials, logging, and checkpoint storage.

Checkpoints are opaque privacy-sensitive bytes. The in-memory store is intended
for examples and tests only; production applications must provide encrypted,
authenticated, device-protected storage.

Native Swift and Kotlin sources are internal package implementation details.
Release assembly supplies prebuilt Apple and Android libraries, so consumer
builds do not require a Rust toolchain.

Repository maintainers use `npm run generate:bindings` after an intentional Rust
ABI change. The command builds the locked runtime, generates Swift and Kotlin
twice, verifies byte-for-byte reproducibility and the exact eight-function ABI,
then updates the reviewed package sources. `npm run check:bindings` performs the
same verification without modifying files and runs in the root quality gate.
