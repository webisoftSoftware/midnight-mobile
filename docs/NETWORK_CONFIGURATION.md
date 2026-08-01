# Network and proof-server configuration

Midnight Mobile recognizes the network identifiers `preview`, `preprod`, and
`mainnet`, but an identifier does not select infrastructure. The package ships
no URL, API key, credential, operated-service default, failover endpoint, or
certificate policy.

## Required configuration

`createStandardMidnightTransport` requires:

```ts
interface MidnightNetworkConfiguration {
  readonly indexerHttpUrl: string;
  readonly indexerWebSocketUrl: string;
  readonly proofServerUrl: string;
  readonly nodeUrl: string;
}
```

The URL protocol constraints are:

| Field                 | Accepted schemes | Role      |
| --------------------- | ---------------- | --------- |
| `indexerHttpUrl`      | `http`, `https`  | `indexer` |
| `indexerWebSocketUrl` | `ws`, `wss`      | `indexer` |
| `proofServerUrl`      | `http`, `https`  | `proof`   |
| `nodeUrl`             | `http`, `https`  | `node`    |

Use `https` and `wss` outside a controlled local test environment. URL syntax
validation does not establish that a service is authentic, compatible, private,
or safe.

`MidnightWalletSessionConfig.networkId` must be `preview`, `preprod`, or
`mainnet`. The runtime validates transactions and checkpoints against that
network. It does not verify that the supplied URLs actually serve the same
network. The adopter must keep session configuration and every endpoint
consistent.

## Injected HTTP contract

The package does not read global `fetch`. Supply a `MidnightFetch`:

```ts
type MidnightFetch = (
  url: string,
  request: {
    readonly method: "POST";
    readonly headers: Readonly<Record<string, string>>;
    readonly body: Uint8Array;
    readonly signal: AbortSignal;
  },
) => Promise<{
  readonly status: number;
  arrayBuffer(): Promise<ArrayBuffer>;
}>;
```

The transport posts `application/octet-stream` to the URL selected by the
runtime's endpoint role. It passes an abort signal covering caller cancellation
and the configured timeout. The implementation must:

- send the provided bytes without text conversion;
- honor abort promptly;
- return the original status;
- expose response bytes through `arrayBuffer`;
- reject on connection or response-body failure; and
- avoid logging URLs, headers, bodies, or credentials.

The default timeout is 30,000 ms. `timeoutMs` and `maximumEffectSteps` must be
positive safe integers; the default effect limit is 64.

## Injected WebSocket contract

The package also does not read a global WebSocket. Supply a
`MidnightWebSocketFactory` that returns the documented minimal socket shape.
`openSyncSocket` connects only to `indexerWebSocketUrl`, accepts binary
`ArrayBuffer` messages or canonical base64 strings, and closes with code `1000`
and reason `cancelled` after caller abort.

The current factory receives only the URL. If the indexer requires credentials,
protocols, cookies, certificate pinning, or platform options, close over them in
the factory. Do not put them in the URL.

## Authentication headers

The optional header callback is evaluated for every HTTP request:

```ts
headers(
  role: "indexer" | "proof" | "node",
): Promise<Readonly<Record<string, string>>>;
```

Resolve the least-privileged credential for that role. Keep credentials in the
application's protected configuration and rotate them according to the service
operator's policy. The callback must not return a conflicting `content-type`;
adopter headers are merged after the SDK's default and therefore can replace it.

The SDK deliberately does not define an authentication scheme. In particular, it
includes no 1AM gateway challenge, session, API key, or private endpoint.

## Proof-server contract

The native runtime emits opaque `check` and `prove` requests for individual
circuits, then a `balance` request containing the locally proved and sealed
transaction. The transport sends each proof-role effect to the configured
proof-server URL. `proveAndBalance` remains decodable for compatibility with an
older native runtime, but current transaction paths do not emit it.

The standard transport appends `/check` or `/prove` for individual proof effects
and `/balance-only` for balancing. A configured path prefix is preserved. The
legacy combined effect routes to `/prove-and-balance`.

The proof server is security-sensitive:

- it observes proof request timing and payload sizes;
- malformed or adversarial responses are decoded and validated by the runtime,
  but may still cause denial of service;
- it can reject, delay, or withhold responses;
- there is no automatic failover or retry; and
- compatibility depends on the exact Ledger revision in the
  [compatibility matrix](./COMPATIBILITY.md).

For `accepted` balance responses, Rust validates the returned transaction,
network, expected identifiers, and transaction hash before committing proposed
wallet state. Any rejected, ambiguous, malformed, or inconsistent proof result
fails with `PROOF_FAILED`.

## Response and retry policy

| Condition                                 | Submission effect | Other effect      |
| ----------------------------------------- | ----------------- | ----------------- |
| HTTP 2xx                                  | accepted          | accepted          |
| HTTP 4xx                                  | rejected          | rejected          |
| HTTP 3xx or 5xx                           | status unknown    | `TRANSPORT_ERROR` |
| Connection, body-read, or timeout failure | status unknown    | `TRANSPORT_ERROR` |
| Caller abort                              | `CANCELLED`       | `CANCELLED`       |

The standard transport performs no automatic retry and no endpoint failover.
This is essential for submissions: a lost response does not prove that a node
rejected the transaction. `statusUnknown` becomes `SUBMISSION_STATUS_UNKNOWN`,
remains in the wallet snapshot/checkpoint, and requires reconciliation by
transaction hash.

Adopters may implement retry around idempotent service operations only after
understanding the selected service's protocol. Do not wrap `submitFinalized` in
a blind retry.

## Environment separation

Maintain separate endpoint and credential records for preview, preprod, and
mainnet. At runtime:

1. select one application-owned network profile;
2. verify every URL and credential belongs to that profile;
3. set the same `networkId` in the wallet session;
4. namespace checkpoint storage by network and wallet; and
5. reject configuration assembled from mixed environments.

The controller's default checkpoint key is `<networkId>:<walletFingerprint>`.
The fingerprint is an app-owned stable identifier, not a secret-storage
substitute and not proof of endpoint identity.

## Logging and diagnostics

The optional SDK logger receives only redaction-safe structured metadata:
endpoint role, effect, outcome, duration, byte length, and stable error code.
Applications must apply the same restriction to their injected network
implementations.

Never record:

- endpoint URLs containing tenant identifiers;
- headers, cookies, tokens, or credentials;
- request or response bodies;
- wallet seeds, checkpoints, addresses tied to a person, or signatures;
- transaction payloads or proof material; or
- unredacted platform crash or HTTP traces.

Use synthetic services from the [Expo example](../examples/expo/README.md) for
routine development. Live preview testing is explicit and must use disposable
wallet material and adopter-supplied endpoints.
