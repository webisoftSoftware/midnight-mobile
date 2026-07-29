import assert from "node:assert/strict";
import test from "node:test";

import type { NativeRuntimeModule } from "../src/native-module.js";
import type { MidnightOperationHandle } from "../src/runtime-types.js";
import { createMidnightRuntimeApi } from "../src/runtime-api.js";

const hashA = "11".repeat(32);
const hashB = "22".repeat(32);
const config = {
  networkId: "preprod",
  walletFingerprint: "wallet",
  unshieldedAddress: "mn_addr",
} as const;

function secrets() {
  return {
    nightExternalKey: new Uint8Array(32).fill(1),
    zswapSeed: new Uint8Array(32).fill(2),
    dustSeed: new Uint8Array(32).fill(3),
  };
}

function snapshotValue(): Record<string, unknown> {
  return {
    networkId: "preprod",
    walletFingerprint: "wallet",
    status: "ready",
    generation: 1,
    streamOffsets: [],
    unshieldedAddress: "mn_addr",
    shieldedCoinPublicKeyHex: hashA,
    shieldedEncryptionPublicKeyHex: hashB,
    dustPublicKey: "1",
    balances: {
      shieldedBalances: {},
      unshieldedBalances: {},
      totalShielded: "0",
      totalUnshielded: "0",
      dustBalance: "0",
      dustCoins: [],
      availableUtxos: 0,
      dustGeneratingNight: "0",
    },
    pendingSubmissions: [],
  };
}

function snapshotJson(): string {
  return JSON.stringify(snapshotValue());
}

function nativeModule(): NativeRuntimeModule {
  return {
    openWalletSession() {
      return Promise.resolve({ id: 1, generation: 1 });
    },
    applySyncBatch() {
      return Promise.resolve(
        JSON.stringify({
          duplicate: false,
          snapshot: snapshotValue(),
        }),
      );
    },
    getWalletSnapshot() {
      return Promise.resolve(snapshotJson());
    },
    exportWalletCheckpoint() {
      return Promise.resolve("AQI=");
    },
    beginCommand() {
      return Promise.resolve(
        JSON.stringify({
          kind: "complete",
          operation: null,
          result: {
            signatureHex: "33".repeat(64),
            verifyingKeyHex: hashA,
          },
        }),
      );
    },
    resumeOperation(operationId, generation) {
      return Promise.resolve(
        JSON.stringify({
          kind: "complete",
          operation: {
            id: operationId,
            generation,
          },
          result: { values: ["1"] },
        }),
      );
    },
    cancelOperation() {
      return Promise.resolve();
    },
    closeWalletSession() {
      return Promise.resolve();
    },
  };
}

interface SyncCall {
  stream: string;
  fromOffset: number;
  toOffset: number;
  payloads: readonly string[];
}

function syncTrackingNative(calls: SyncCall[]): NativeRuntimeModule {
  const caughtUp = new Set<string>();
  return {
    ...nativeModule(),
    applySyncBatch(
      _sessionId,
      _generation,
      stream,
      fromOffset,
      toOffset,
      payloads,
    ) {
      calls.push({ stream, fromOffset, toOffset, payloads });
      if (stream.endsWith("-tip")) caughtUp.add(stream.slice(0, -4));
      return Promise.resolve(
        JSON.stringify({
          duplicate: stream === "unshielded",
          snapshot: {
            ...snapshotValue(),
            status: caughtUp.size === 3 ? "ready" : "syncing",
          },
        }),
      );
    },
  };
}

const expectedSyncCalls: readonly SyncCall[] = [
  {
    stream: "shielded",
    fromOffset: 0,
    toOffset: 1,
    payloads: ["AQ=="],
  },
  {
    stream: "shielded-tip",
    fromOffset: 1,
    toOffset: 1,
    payloads: [],
  },
  {
    stream: "unshielded",
    fromOffset: 0,
    toOffset: 1,
    payloads: ["AQ=="],
  },
  {
    stream: "unshielded-tip",
    fromOffset: 1,
    toOffset: 1,
    payloads: [],
  },
  {
    stream: "dust",
    fromOffset: 0,
    toOffset: 1,
    payloads: ["AQ=="],
  },
  {
    stream: "dust-tip",
    fromOffset: 1,
    toOffset: 1,
    payloads: [],
  },
];

await test("runtime API copies and wipes secrets around native open", async () => {
  const captured: Uint8Array[] = [];
  let checkpoint = "";
  const native = {
    ...nativeModule(),
    openWalletSession(
      _configJson: string,
      night: Uint8Array,
      zswap: Uint8Array,
      dust: Uint8Array,
      checkpointBase64: string | null,
    ) {
      captured.push(night, zswap, dust);
      checkpoint = checkpointBase64 ?? "";
      return Promise.resolve({ id: 1, generation: 1 });
    },
  };
  const input = secrets();
  const api = createMidnightRuntimeApi(() => native);
  const handle = await api.openWalletSession(config, input, {
    version: 1,
    bytes: new Uint8Array([4]),
  });
  assert.deepEqual(handle, { id: 1, generation: 1 });
  assert.equal(checkpoint, "BA==");
  assert.deepEqual(
    captured.map((value) => [...value]),
    [[], [], []].map(() => Array.from({ length: 32 }, () => 0)),
  );
  assert.equal(input.nightExternalKey[0], 1);
  await api.openWalletSession(config, secrets());
});

await test("runtime API maps every remaining native ABI method", async () => {
  const native = nativeModule();
  const api = createMidnightRuntimeApi(() => native);
  const session = { id: 1, generation: 1 };
  const applied = await api.applySyncBatch(session, {
    stream: "shielded",
    fromOffset: 0,
    toOffset: 1,
    payloads: [new Uint8Array([1])],
  });
  assert.equal(applied.duplicate, false);
  assert.equal((await api.getWalletSnapshot(session)).status, "ready");
  assert.deepEqual(
    (await api.exportWalletCheckpoint(session)).bytes,
    new Uint8Array([1, 2]),
  );
  const begun = await api.beginCommand(session, {
    kind: "signData",
    domain: "test",
    dataBase64: "",
  });
  assert.equal(begun.kind, "complete");
  const operation: MidnightOperationHandle<"parseCheckResult"> = {
    id: 2,
    generation: 1,
    commandKind: "parseCheckResult",
  };
  const resumed = await api.resumeOperation(operation, {
    effectId: "effect",
    outcome: "accepted",
  });
  assert.equal(resumed.kind, "complete");
  await api.cancelOperation(operation);
  await api.closeWalletSession(session);
});

await test("only terminal empty batches mark public streams caught up", async () => {
  const calls: SyncCall[] = [];
  const native = syncTrackingNative(calls);
  const api = createMidnightRuntimeApi(() => native);
  const session = { id: 1, generation: 1 };

  for (const stream of ["shielded", "unshielded", "dust"] as const) {
    const applied = await api.applySyncBatch(session, {
      stream,
      fromOffset: 0,
      toOffset: 1,
      payloads: [Uint8Array.of(1)],
    });
    assert.equal(applied.duplicate, stream === "unshielded");
    assert.equal(applied.snapshot.status, "syncing");
    const terminal = await api.applySyncBatch(session, {
      stream,
      fromOffset: 1,
      toOffset: 1,
      payloads: [],
    });
    assert.equal(terminal.duplicate, false);
    assert.equal(
      terminal.snapshot.status,
      stream === "dust" ? "ready" : "syncing",
    );
  }

  assert.deepEqual(calls, expectedSyncCalls);
});

await test("sync batches reject unknown streams and native failures", async () => {
  let calls = 0;
  const native = {
    ...nativeModule(),
    applySyncBatch() {
      calls += 1;
      return Promise.reject(new Error("SYNC_GAP"));
    },
  };
  const api = createMidnightRuntimeApi(() => native);
  const session = { id: 1, generation: 1 };
  await assert.rejects(
    api.applySyncBatch(session, {
      stream: "unknown",
      fromOffset: 0,
      toOffset: 0,
      payloads: [],
    } as never),
    { code: "INVALID_ARGUMENT" },
  );
  assert.equal(calls, 0);
  await assert.rejects(
    api.applySyncBatch(session, {
      stream: "dust",
      fromOffset: 0,
      toOffset: 1,
      payloads: [],
    }),
    { code: "SYNC_GAP" },
  );
  assert.equal(calls, 1);
});

await test("runtime API rejects unavailable native modules and invalid secrets", async () => {
  const unavailable = createMidnightRuntimeApi(() => {
    throw new Error("missing");
  });
  await assert.rejects(unavailable.openWalletSession(config, secrets()), {
    code: "UNAVAILABLE",
  });
  const native = nativeModule();
  const incomplete = createMidnightRuntimeApi(() => ({
    openWalletSession(
      ...arguments_: Parameters<NativeRuntimeModule["openWalletSession"]>
    ) {
      return native.openWalletSession(...arguments_);
    },
  }));
  await assert.rejects(incomplete.openWalletSession(config, secrets()), {
    code: "UNAVAILABLE",
  });
  const api = createMidnightRuntimeApi(() => nativeModule());
  await assert.rejects(
    api.openWalletSession(config, {
      ...secrets(),
      dustSeed: new Uint8Array(31),
    }),
    { code: "INVALID_ARGUMENT" },
  );
  const defaultApi = createMidnightRuntimeApi();
  await assert.rejects(defaultApi.openWalletSession(config, secrets()), {
    code: "UNAVAILABLE",
  });
});

await test("runtime API rejects invalid handles and sync ranges", async () => {
  const api = createMidnightRuntimeApi(() => nativeModule());
  for (const handle of [
    { id: -1, generation: 1 },
    { id: 1.5, generation: 1 },
  ]) {
    await assert.rejects(api.getWalletSnapshot(handle), {
      code: "INVALID_ARGUMENT",
    });
  }
  for (const { fromOffset, toOffset } of [
    { fromOffset: -1, toOffset: 0 },
    { fromOffset: 2, toOffset: 1 },
    { fromOffset: 0.5, toOffset: 1 },
    { fromOffset: 0, toOffset: 1.5 },
  ]) {
    await assert.rejects(
      api.applySyncBatch(
        { id: 1, generation: 1 },
        {
          stream: "dust",
          fromOffset,
          toOffset,
          payloads: [],
        },
      ),
      { code: "INVALID_ARGUMENT" },
    );
  }
});

await test("runtime API normalizes native rejections", async () => {
  const api = createMidnightRuntimeApi(() => ({
    ...nativeModule(),
    closeWalletSession() {
      return Promise.reject(
        Object.assign(new Error("native rejected"), { code: "staleSession" }),
      );
    },
  }));
  await assert.rejects(api.closeWalletSession({ id: 1, generation: 1 }), {
    code: "STALE_SESSION",
  });
});

await test("public index loads every production entrypoint", async () => {
  const entrypoint = await import("../src/index.js");
  assert.equal(entrypoint.MIDNIGHT_COMMAND_KINDS.length, 19);
  assert.equal(
    entrypoint.EXPO_MIDNIGHT_NATIVE_MODULE_NAME,
    "ExpoMidnightNative",
  );
  assert.equal(typeof entrypoint.MidnightRuntimeProvider, "function");
  assert.equal(typeof entrypoint.createMidnightRuntimeApi, "function");
});
