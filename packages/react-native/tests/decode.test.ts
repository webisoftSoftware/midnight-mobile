import assert from "node:assert/strict";
import test from "node:test";

import {
  decodeApplySync,
  decodeCommandResult,
  decodeOperationStep,
  decodeSessionHandle,
  decodeWalletSnapshot,
  parseNativeJson,
} from "../src/decode.js";
import { MidnightRuntimeError } from "../src/errors.js";

const hashA = "11".repeat(32);
const hashB = "22".repeat(32);
const finalized = {
  transactionBase64: "AA==",
  transactionHash: hashA,
  ledgerTransactionHash: hashB,
  identifiers: ["aa", "bb"],
  expiresAt: 42,
};

function snapshotValue(): Record<string, unknown> {
  return {
    networkId: "preprod",
    walletFingerprint: "wallet",
    status: "ready",
    generation: 3,
    streamOffsets: [
      { stream: "shielded", nextOffset: 1 },
      { stream: "unshielded", nextOffset: 2 },
      { stream: "dust", nextOffset: 3 },
    ],
    unshieldedAddress: "mn_addr",
    shieldedCoinPublicKeyHex: hashA,
    shieldedEncryptionPublicKeyHex: hashB,
    dustPublicKey: "12345678901234567890",
    balances: {
      shieldedBalances: { aa: "1" },
      unshieldedBalances: { bb: "2" },
      totalShielded: "1",
      totalUnshielded: "2",
      dustBalance: "3",
      dustCoins: [
        {
          nonce: "4",
          generatedNow: "5",
          maxCap: "6",
          maxCapReachedAt: null,
          maturing: true,
        },
        {
          nonce: "7",
          generatedNow: "0",
          maxCap: "8",
          maxCapReachedAt: "2026-07-29T00:00:00Z",
          maturing: false,
        },
      ],
      availableUtxos: 2,
      dustGeneratingNight: "9",
    },
    pendingSubmissions: [
      {
        transactionHash: hashA,
        identifiers: ["cc"],
        status: "awaitingResponse",
      },
      {
        transactionHash: hashB,
        identifiers: [],
        status: "accepted",
      },
      {
        transactionHash: hashA,
        identifiers: ["dd"],
        status: "rejected",
      },
      {
        transactionHash: hashB,
        identifiers: ["ee"],
        status: "statusUnknown",
      },
    ],
  };
}

function assertNativeFailure(action: () => unknown): void {
  assert.throws(action, (error: unknown) => {
    return (
      error instanceof MidnightRuntimeError && error.code === "NATIVE_INTERNAL"
    );
  });
}

await test("snapshot, sync, handle, and JSON decoders accept valid values", () => {
  const snapshot = decodeWalletSnapshot(snapshotValue());
  assert.equal(snapshot.networkId, "preprod");
  assert.equal(snapshot.balances.dustCoins[0]?.maxCapReachedAt, null);
  assert.equal(
    snapshot.balances.dustCoins[1]?.maxCapReachedAt,
    "2026-07-29T00:00:00Z",
  );
  assert.deepEqual(
    snapshot.pendingSubmissions.map(({ status }) => status),
    ["awaitingResponse", "accepted", "rejected", "statusUnknown"],
  );
  assert.deepEqual(decodeSessionHandle({ id: 7, generation: 8 }), {
    id: 7,
    generation: 8,
  });
  assert.equal(
    decodeApplySync({ duplicate: true, snapshot: snapshotValue() }).duplicate,
    true,
  );
  assert.deepEqual(parseNativeJson('{"ok":true}'), { ok: true });
});

await test("snapshot decoders reject malformed containers and discriminants", () => {
  for (const value of [null, [], "wallet"]) {
    assertNativeFailure(() => decodeWalletSnapshot(value));
  }
  for (const [key, value] of [
    ["networkId", "testnet"],
    ["status", "idle"],
    ["streamOffsets", {}],
    ["generation", -1],
    ["walletFingerprint", 7],
  ] as const) {
    assertNativeFailure(() =>
      decodeWalletSnapshot({ ...snapshotValue(), [key]: value }),
    );
  }
  assertNativeFailure(() => parseNativeJson("{"));
});

await test("snapshot decoders reject malformed nested wallet state", () => {
  const valid = snapshotValue();
  const balances = valid.balances as Record<string, unknown>;
  const failures: readonly (() => unknown)[] = [
    () =>
      decodeWalletSnapshot({
        ...valid,
        streamOffsets: [{ stream: "other", nextOffset: 0 }],
      }),
    () =>
      decodeWalletSnapshot({
        ...valid,
        streamOffsets: [{ stream: "dust", nextOffset: 0.5 }],
      }),
    () =>
      decodeWalletSnapshot({
        ...valid,
        balances: { ...balances, totalShielded: "01" },
      }),
    () =>
      decodeWalletSnapshot({
        ...valid,
        balances: { ...balances, shieldedBalances: { AA: "1" } },
      }),
    () =>
      decodeWalletSnapshot({
        ...valid,
        balances: { ...balances, dustCoins: "none" },
      }),
    () =>
      decodeWalletSnapshot({
        ...valid,
        pendingSubmissions: [
          { transactionHash: hashA, identifiers: [], status: "pending" },
        ],
      }),
  ];
  for (const failure of failures) assertNativeFailure(failure);
});

await test("all correlated command results decode", () => {
  assert.equal(
    decodeCommandResult("signData", {
      signatureHex: "33".repeat(64),
      verifyingKeyHex: hashA,
    }).verifyingKeyHex,
    hashA,
  );
  assert.deepEqual(
    decodeCommandResult("createCheckPayload", {
      payloadBase64: "",
    }),
    { payloadBase64: "" },
  );
  assert.deepEqual(
    decodeCommandResult("parseCheckResult", {
      values: [null, "0", "18446744073709551615"],
    }).values,
    [null, "0", "18446744073709551615"],
  );
  assert.equal(
    decodeCommandResult("createProvingPayload", {
      payloadBase64: "AQ==",
    }).payloadBase64,
    "AQ==",
  );
  assert.equal(
    decodeCommandResult("canonicalizeTransaction", {
      transactionBase64: "AgM=",
    }).transactionBase64,
    "AgM=",
  );
  assert.deepEqual(
    decodeCommandResult("createSyncRequest", {
      stream: "dust",
      fromOffset: 4,
      requestBase64: "BA==",
    }),
    { stream: "dust", fromOffset: 4, requestBase64: "BA==" },
  );
  assert.deepEqual(
    decodeCommandResult("deriveShieldedMintContext", {
      coinPublicKeyHex: hashA,
      encryptionPublicKeyHex: hashB,
      outputIndex: 4,
    }),
    {
      coinPublicKeyHex: hashA,
      encryptionPublicKeyHex: hashB,
      outputIndex: 4,
    },
  );
  assert.deepEqual(
    decodeCommandResult("watchShieldedMint", { outputIndex: 5 }),
    { outputIndex: 5 },
  );
});

await test("shielded and dust command results decode every status", () => {
  assert.deepEqual(
    decodeCommandResult("createShieldedSpentRequest", {
      requestBase64: "BQ==",
      nullifierCount: 2,
    }),
    { requestBase64: "BQ==", nullifierCount: 2 },
  );
  assert.deepEqual(
    decodeCommandResult("applyShieldedSpentResponse", { removedCount: 1 }),
    { removedCount: 1 },
  );
  assert.deepEqual(
    decodeCommandResult("setShieldedProtocolVersion", { protocolVersion: 1 }),
    { protocolVersion: 1 },
  );
  assert.deepEqual(
    decodeCommandResult("createDustSpendRequest", {
      requestBase64: "Bg==",
      utxoCount: 3,
    }),
    { requestBase64: "Bg==", utxoCount: 3 },
  );
  assert.deepEqual(
    decodeCommandResult("createDustCommitmentRequest", { status: "ahead" }),
    { status: "ahead" },
  );
  assert.deepEqual(
    decodeCommandResult("createDustCommitmentRequest", {
      status: "unchanged",
    }),
    { status: "unchanged" },
  );
  assert.deepEqual(
    decodeCommandResult("createDustCommitmentRequest", {
      status: "rebuild",
      requestBase64: "Bw==",
    }),
    { status: "rebuild", requestBase64: "Bw==" },
  );
  assert.deepEqual(
    decodeCommandResult("applyDustSpendResolution", { status: "applied" }),
    { status: "applied" },
  );
});

await test("all finalized and submission results decode", () => {
  const finalizedKinds = [
    "transfer",
    "dappTransfer",
    "dappIntent",
    "generateDust",
    "balanceUnsealed",
    "balanceSealed",
    "finalizeUnprovenTransaction",
  ] as const;
  for (const kind of finalizedKinds) {
    const result = decodeCommandResult(kind, finalized);
    assert.equal(result.transactionHash, hashA);
    assert.equal(result.expiresAt, 42);
  }
  const withoutExpiry: Record<string, unknown> = { ...finalized };
  Reflect.deleteProperty(withoutExpiry, "expiresAt");
  assert.equal(
    decodeCommandResult("transfer", withoutExpiry).expiresAt,
    undefined,
  );
  assert.deepEqual(
    decodeCommandResult("submitFinalized", {
      transactionHash: hashA,
      identifiers: ["aa"],
      status: "accepted",
      bodyBase64: "CA==",
    }),
    {
      transactionHash: hashA,
      identifiers: ["aa"],
      status: "accepted",
      bodyBase64: "CA==",
    },
  );
  assert.equal(
    decodeCommandResult("submitFinalized", {
      transactionHash: hashB,
      identifiers: [],
      status: "rejected",
    }).status,
    "rejected",
  );
});

await test("command result decoders reject malformed wire values", () => {
  const failures: readonly (() => unknown)[] = [
    () => decodeCommandResult("parseCheckResult", { values: "0" }),
    () => decodeCommandResult("parseCheckResult", { values: ["01"] }),
    () => decodeCommandResult("createCheckPayload", { payloadBase64: "AB==" }),
    () =>
      decodeCommandResult("createSyncRequest", {
        stream: "shielded",
        fromOffset: -1,
        requestBase64: "",
      }),
    () =>
      decodeCommandResult("deriveShieldedMintContext", {
        coinPublicKeyHex: "AA".repeat(32),
        encryptionPublicKeyHex: hashB,
        outputIndex: 0,
      }),
    () =>
      decodeCommandResult("createDustCommitmentRequest", {
        status: "rebuild",
        requestBase64: 4,
      }),
    () =>
      decodeCommandResult("createDustCommitmentRequest", {
        status: "unknown",
      }),
    () =>
      decodeCommandResult("applyDustSpendResolution", { status: "pending" }),
    () =>
      decodeCommandResult("transfer", {
        ...finalized,
        expiresAt: Number.MAX_SAFE_INTEGER + 1,
      }),
    () =>
      decodeCommandResult("submitFinalized", {
        transactionHash: hashA,
        identifiers: [],
        status: "statusUnknown",
      }),
  ];
  for (const failure of failures) assertNativeFailure(failure);
});

await test("operation steps decode complete, progress, and network variants", () => {
  const handle = { id: 4, generation: 5 };
  assert.equal(
    decodeOperationStep("canonicalizeTransaction", {
      kind: "complete",
      operation: null,
      result: { transactionBase64: "" },
    }).kind,
    "complete",
  );
  assert.deepEqual(
    decodeOperationStep("signData", {
      kind: "progress",
      operation: handle,
      phase: "proving",
      percent: 50,
    }).operation,
    { ...handle, commandKind: "signData" },
  );
  const effects = [
    "sync",
    "check",
    "prove",
    "proveAndBalance",
    "balance",
    "submit",
    "confirm",
  ] as const;
  const roles = ["indexer", "proof", "node"] as const;
  for (const [index, effect] of effects.entries()) {
    const step = decodeOperationStep("transfer", {
      kind: "network",
      operation: handle,
      effectId: String(index),
      effect,
      endpointRole: roles[index % roles.length],
      bodyBase64: "",
    });
    assert.equal(step.kind, "network");
  }
  assert.equal(
    decodeOperationStep("transfer", {
      kind: "progress",
      operation: handle,
      phase: "queued",
    }).kind,
    "progress",
  );
});

await test("operation decoders reject malformed steps", () => {
  const handle = { id: 4, generation: 5 };
  const failures: readonly (() => unknown)[] = [
    () =>
      decodeOperationStep("transfer", { kind: "progress", operation: null }),
    () =>
      decodeOperationStep("transfer", {
        kind: "network",
        operation: handle,
        effectId: "1",
        effect: "privateSync",
        endpointRole: "node",
        bodyBase64: "",
      }),
    () =>
      decodeOperationStep("transfer", {
        kind: "network",
        operation: handle,
        effectId: "1",
        effect: "submit",
        endpointRole: "gateway",
        bodyBase64: "",
      }),
    () =>
      decodeOperationStep("transfer", {
        kind: "progress",
        operation: handle,
        phase: "proving",
        percent: -1,
      }),
    () =>
      decodeOperationStep("transfer", {
        kind: "mystery",
        operation: handle,
      }),
  ];
  for (const failure of failures) assertNativeFailure(failure);
});
