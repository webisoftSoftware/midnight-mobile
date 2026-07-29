# Checkpoint storage and encryption contract

A `MidnightCheckpoint` is an opaque, versioned byte sequence representing
privacy-sensitive wallet state. It is not a portable JSON model, backup format,
credential container, or stable cross-version interchange format.

```ts
interface MidnightCheckpoint {
  readonly version: 1;
  readonly bytes: Uint8Array;
}
```

Applications may store and return the bytes. They must not inspect, edit, merge,
compress, partially restore, or construct them.

## What a checkpoint preserves

The current native checkpoint records enough wallet state to restore:

- the selected network and wallet fingerprint;
- native wallet state;
- next offsets and caught-up state for shielded, unshielded, and DUST streams;
- bounded recent sync-batch receipts;
- pending submission hashes, identifiers, and statuses;
- the exact Ledger source revision; and
- the native generation fence.

The representation is deliberately private. This list describes operational
behavior, not a schema that adopters may depend on.

Checkpoint content can reveal wallet activity, balances, addresses, sync
position, transaction relationships, and pending submissions. Treat the entire
blob as confidential even if a future inspection suggests that a particular
field is public.

## Native validation is not storage security

The current encoding includes magic bytes, a format version, a Ledger revision,
and an internal SHA-256 checksum. Restore also validates its structure, network,
wallet fingerprint, streams, receipts, and pending submissions.

These checks detect accidental corruption and incompatibility. They provide no
authenticity or confidentiality: an attacker who can read or replace storage can
learn state, replay an older valid checkpoint, or construct a new checksum. The
internal checksum is not a MAC and is not a substitute for authenticated
encryption.

Native checkpoint size is limited to 64 MiB. Applications should impose a
smaller operational bound appropriate to their storage before allocation or
decryption.

## Store interface

Inject a `MidnightCheckpointStore` into the controller or provider:

```ts
interface MidnightCheckpointStore {
  load(key: string): Promise<MidnightCheckpoint | null>;
  save(key: string, checkpoint: MidnightCheckpoint): Promise<void>;
  remove(key: string): Promise<void>;
}
```

The controller uses `<networkId>:<walletFingerprint>` as its key. The
application must ensure both components are stable, collision-free within its
namespace, and not exposed in logs. `load` returns `null` only when no
checkpoint exists. Corrupt, undecryptable, rolled-back, or inaccessible records
must be reported as errors rather than silently treated as a new wallet.

The controller calls `save` after a non-duplicate sync batch, at each command
step, on pause, and before close. Implement `save` as an atomic replace:

1. encrypt a complete new record;
2. durably write it to a temporary or transactional location;
3. verify that the write completed; and
4. atomically make it current.

A failed save must leave the last authenticated record readable. The controller
does not call `remove` automatically. Deletion, logout, and retention behavior
belong to the application.

`InMemoryMidnightCheckpointStore` clones bytes, has process lifetime only, and
is provided solely for tests and examples. It is neither encrypted nor
persistent.

Native mutation and host storage are not one atomic transaction. A sync batch or
command step may already have changed native state before `save` rejects; the
controller cannot roll that state back. Treat a store failure as a durability
incident: stop new wallet work, preserve the last authenticated record, repair
the store, and follow an application recovery path. Design `save` so ordinary
transient conditions are resolved inside the adapter without leaking plaintext
or silently discarding the write.

## Production encryption requirements

A production store must provide authenticated, device-protected encryption. At
minimum:

- use a reviewed AEAD construction with a unique nonce for every write;
- generate and retain the encryption key through the platform's protected key
  service, not JavaScript source, environment variables, or ordinary app
  preferences;
- bind application ID, network ID, wallet fingerprint, checkpoint envelope
  version, and storage key as authenticated associated data;
- include an application-owned monotonically increasing record revision or
  equivalent rollback signal;
- reject authentication failure without returning plaintext;
- keep plaintext only for the minimum load/save duration and wipe mutable
  buffers on every path;
- exclude plaintext and keys from logs, analytics, crash reports, clipboard,
  screenshots, and debugging tools; and
- define backup, device migration, account recovery, lock-screen, rooted or
  jailbroken device, and secure-deletion policy.

Platform examples of a key-protection boundary include an access-controlled
Keychain/Secure Enclave key on Apple platforms and a non-exportable Android
Keystore key. The SDK intentionally does not choose algorithms, access-control
flags, biometric policy, backup accessibility, or a storage library for the
adopter.

Do not use AsyncStorage, plain files, SQLite plaintext, unencrypted cloud
backup, or base64 encoding as production protection.

## Restore behavior

The controller's third `openWalletSession` argument has three distinct states:

- omitted: ask the injected store to load the default key;
- `null`: explicitly start without a checkpoint; or
- a checkpoint: restore exactly those bytes.

Restore requires the checkpoint's network and wallet fingerprint to match the
new session configuration and its Ledger revision to match the runtime.
Malformed, tampered, wrong-wallet, wrong-network, unsupported-version, or
wrong-revision bytes fail with `STATE_INCOMPATIBLE`.

Do not catch `STATE_INCOMPATIBLE` and silently create an empty production
wallet. Quarantine the encrypted record, retain redacted diagnostics, notify the
user without exposing state, and follow an explicit recovery policy.

There is no legacy/plaintext checkpoint migration, automatic schema migration,
or downgrade conversion in this alpha. Operation handles are not checkpointed
resume tokens. After restore:

1. obtain a fresh session handle;
2. read the snapshot and stream offsets;
3. reconcile every `statusUnknown` or `awaitingResponse` submission;
4. resume indexer synchronization contiguously; and
5. begin new commands only after the wallet is caught up.

## Rollback and concurrency

Authenticated encryption alone does not prevent an attacker or backup service
from replaying an older valid record. If rollback matters to the application,
compare an authenticated record revision against a protected monotonic value or
trusted external record. Define what happens when that value is unavailable.

Do not let two processes or two devices write the same store key concurrently.
The runtime supports two in-process sessions, but it does not merge divergent
checkpoints. Serialize access per network/wallet key and detect stale writes in
the storage envelope.

## Export, backup, and deletion

If the application lets a user export or back up wallet state:

- wrap the opaque bytes in a separately versioned, authenticated application
  envelope;
- explain that compatibility is limited to the SDK versions in the release
  record;
- never email, paste, log, or upload plaintext;
- protect user-chosen recovery secrets against offline guessing with an
  appropriate KDF;
- verify restoration using a disposable environment before relying on it; and
- preserve rollback and retention metadata.

Flash storage, journaling filesystems, cloud synchronization, and managed
runtimes make guaranteed physical deletion difficult. `remove` should revoke
keys and delete records, but documentation and UI must not promise forensic
erasure unless the platform design supports that claim.

## Alpha upgrade contract

Checkpoint compatibility can change between alpha releases. Pin the SDK version,
keep the prior authenticated checkpoint until the upgrade succeeds, and review
the release's [compatibility matrix](./COMPATIBILITY.md) and
[upgrade policy](./ALPHA_SUPPORT.md). No downgrade support is promised.
