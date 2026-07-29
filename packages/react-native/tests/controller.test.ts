import assert from "node:assert/strict";
import test from "node:test";

import type {
  MidnightCommand,
  MidnightCommandKind,
  MidnightCommandResult,
} from "../src/commands.js";
import { MidnightRuntimeController } from "../src/controller.js";
import { decodeCommandResult } from "../src/decode.js";
import { MidnightRuntimeError } from "../src/errors.js";
import {
  InMemoryMidnightCheckpointStore,
  type MidnightCheckpointStore,
  type MidnightLogger,
} from "../src/host.js";
import type {
  MidnightOperationStep,
  MidnightRuntimeApi,
  MidnightRuntimeStatus,
  MidnightWalletSnapshot,
} from "../src/runtime-types.js";
import type {
  MidnightRunCommandOptions,
  MidnightStandardTransport,
} from "../src/transport.js";

const snapshot: MidnightWalletSnapshot = {
  networkId: "preprod",
  walletFingerprint: "synthetic-wallet",
  status: "ready",
  generation: 1,
  streamOffsets: [],
  unshieldedAddress: "mn_addr_preprod1synthetic",
  shieldedCoinPublicKeyHex: "11".repeat(32),
  shieldedEncryptionPublicKeyHex: "22".repeat(32),
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

function runtimeApi(onClose: () => void): MidnightRuntimeApi {
  return {
    openWalletSession() {
      return Promise.resolve({ id: 1, generation: 1 });
    },
    applySyncBatch() {
      return Promise.resolve({ duplicate: false, snapshot });
    },
    getWalletSnapshot() {
      return Promise.resolve(snapshot);
    },
    exportWalletCheckpoint() {
      return Promise.resolve({
        version: 1,
        bytes: new Uint8Array([1, 2, 3]),
      });
    },
    beginCommand<K extends MidnightCommandKind>(
      _session: { readonly id: number; readonly generation: number },
      _command: MidnightCommand<K>,
    ): Promise<MidnightOperationStep<K>> {
      return Promise.reject(new Error("unused"));
    },
    resumeOperation() {
      return Promise.reject(new Error("unused"));
    },
    cancelOperation() {
      return Promise.resolve();
    },
    closeWalletSession() {
      onClose();
      return Promise.resolve();
    },
  };
}

const transport: MidnightStandardTransport = {
  runCommand<K extends MidnightCommandKind>(
    _api: MidnightRuntimeApi,
    _session: { readonly id: number; readonly generation: number },
    _command: MidnightCommand<K>,
  ): Promise<MidnightCommandResult<K>> {
    return Promise.reject(new Error("unused"));
  },
  openSyncSocket() {
    throw new Error("unused");
  },
};

const config = {
  networkId: "preprod",
  walletFingerprint: "synthetic-wallet",
  unshieldedAddress: "mn_addr_preprod1synthetic",
} as const;

const secrets = {
  nightExternalKey: new Uint8Array(32),
  zswapSeed: new Uint8Array(32),
  dustSeed: new Uint8Array(32),
};

await test("controller restores, persists, and closes an injected session", async () => {
  let closes = 0;
  const store = new InMemoryMidnightCheckpointStore();
  const controller = new MidnightRuntimeController({
    api: runtimeApi(() => {
      closes += 1;
    }),
    transport,
    checkpointStore: store,
  });
  const session = await controller.openWalletSession(config, secrets);
  assert.equal(controller.status, "ready");

  await controller.applySyncBatch(session, {
    stream: "shielded",
    fromOffset: 0,
    toOffset: 1,
    payloads: [new Uint8Array([7])],
  });
  const saved = await store.load("preprod:synthetic-wallet");
  assert.deepEqual(saved?.bytes, new Uint8Array([1, 2, 3]));

  await controller.closeWalletSession(session);
  assert.equal(closes, 1);
  assert.equal(controller.status, "idle");
});

await test("controller reports status, duplicate sync, refresh, and stale handles", async () => {
  const statuses: MidnightRuntimeStatus[] = [];
  const loggerEvents: object[] = [];
  const logger: MidnightLogger = {
    write(event) {
      loggerEvents.push(event);
    },
  };
  const api: MidnightRuntimeApi = {
    ...runtimeApi(() => undefined),
    applySyncBatch() {
      return Promise.resolve({ duplicate: true, snapshot });
    },
  };
  const controller = new MidnightRuntimeController({
    api,
    transport,
    logger,
  });
  const unsubscribe = controller.subscribe((status) => {
    statuses.push(status);
  });
  const session = await controller.openWalletSession(config, secrets, null);
  await controller.applySyncBatch(session, {
    stream: "dust",
    fromOffset: 0,
    toOffset: 0,
    payloads: [],
  });
  assert.equal((await controller.getWalletSnapshot(session)).status, "ready");
  await controller.refresh();
  unsubscribe();
  await controller.closeWalletSession(session);
  await controller.refresh();
  assert.deepEqual(statuses.slice(0, 4), [
    "opening",
    "ready",
    "syncing",
    "ready",
  ]);
  assert.equal(controller.status, "idle");
  assert.equal(loggerEvents.length >= 3, true);
  assert.throws(() => controller.getWalletSnapshot(session), {
    code: "STALE_SESSION",
  });
});

await test("controller tracks command steps and cancels an active operation", async () => {
  let cancelled = 0;
  let releaseStart: (() => void) | undefined;
  const started = new Promise<void>((resolve) => {
    releaseStart = resolve;
  });
  const api = runtimeApi(() => undefined);
  const activeTransport: MidnightStandardTransport = {
    async runCommand<K extends MidnightCommandKind>(
      _api: MidnightRuntimeApi,
      _session: { readonly id: number; readonly generation: number },
      command: MidnightCommand<K>,
      options?: MidnightRunCommandOptions<K>,
    ): Promise<MidnightCommandResult<K>> {
      const operation = { id: 9, generation: 1, commandKind: command.kind };
      await options?.onStep?.({
        kind: "network",
        operation,
        effectId: "effect",
        effect: "submit",
        endpointRole: "node",
        bodyBase64: "",
      });
      releaseStart?.();
      return new Promise((_resolve, reject) => {
        options?.signal?.addEventListener(
          "abort",
          () => {
            reject(new MidnightRuntimeError("CANCELLED"));
          },
          { once: true },
        );
      });
    },
    openSyncSocket() {
      throw new Error("unused");
    },
  };
  const controller = new MidnightRuntimeController({
    api: {
      ...api,
      cancelOperation() {
        cancelled += 1;
        return Promise.reject(new Error("already complete"));
      },
    },
    transport: activeTransport,
    checkpointStore: new InMemoryMidnightCheckpointStore(),
  });
  const session = await controller.openWalletSession(config, secrets);
  const running = controller.runCommand(session, {
    kind: "submitFinalized",
    rawBase64: "",
  });
  await started;
  await controller.pause();
  await assert.rejects(running, { code: "CANCELLED" });
  assert.equal(cancelled, 1);
});

await test("controller tracks progress and completion and relays caller abort", async () => {
  const outer = new AbortController();
  const stepKinds: string[] = [];
  const commandTransport: MidnightStandardTransport = {
    async runCommand<K extends MidnightCommandKind>(
      _api: MidnightRuntimeApi,
      _session: { readonly id: number; readonly generation: number },
      command: MidnightCommand<K>,
      options?: MidnightRunCommandOptions<K>,
    ): Promise<MidnightCommandResult<K>> {
      const operation = { id: 5, generation: 1, commandKind: command.kind };
      const result = decodeCommandResult(command.kind, {
        transactionHash: "aa".repeat(32),
        identifiers: [],
        status: "accepted",
      });
      await options?.onStep?.({
        kind: "progress",
        operation,
        phase: "queued",
      });
      await options?.onStep?.({
        kind: "complete",
        operation,
        result,
      });
      outer.abort();
      assert.equal(options?.signal?.aborted, true);
      return result;
    },
    openSyncSocket() {
      throw new Error("unused");
    },
  };
  const controller = new MidnightRuntimeController({
    api: runtimeApi(() => undefined),
    transport: commandTransport,
  });
  const session = await controller.openWalletSession(config, secrets);
  const result = await controller.runCommand(
    session,
    { kind: "submitFinalized", rawBase64: "" },
    {
      signal: outer.signal,
      onStep(step) {
        stepKinds.push(step.kind);
      },
    },
  );
  assert.equal(result.status, "accepted");
  assert.deepEqual(stepKinds, ["progress", "complete"]);
  await controller.pause();
});

await test("controller closes after persistence failure and normalizes open failure", async () => {
  let closed = false;
  const failingStore: MidnightCheckpointStore = {
    load() {
      return Promise.resolve(null);
    },
    save() {
      return Promise.reject(new Error("disk failed"));
    },
    remove() {
      return Promise.resolve();
    },
  };
  const api = runtimeApi(() => {
    closed = true;
  });
  const controller = new MidnightRuntimeController({
    api,
    transport,
    checkpointStore: failingStore,
  });
  const session = await controller.openWalletSession(config, secrets, {
    version: 1,
    bytes: new Uint8Array(),
  });
  await assert.rejects(controller.closeWalletSession(session), {
    code: "NATIVE_INTERNAL",
  });
  assert.equal(closed, true);
  assert.equal(controller.status, "idle");

  const rejectedApi: MidnightRuntimeApi = {
    ...api,
    openWalletSession(): Promise<never> {
      return Promise.reject(new MidnightRuntimeError("UNAVAILABLE"));
    },
  };
  const rejected = new MidnightRuntimeController({
    api: rejectedApi,
    transport,
  });
  await assert.rejects(rejected.openWalletSession(config, secrets), {
    code: "UNAVAILABLE",
  });
  assert.equal(rejected.status, "error");
});

await test("dispose closes all sessions and reports any close failure", async () => {
  let nextId = 0;
  const api: MidnightRuntimeApi = {
    ...runtimeApi(() => undefined),
    openWalletSession() {
      nextId += 1;
      return Promise.resolve({ id: nextId, generation: 1 });
    },
    closeWalletSession(): Promise<never> {
      return Promise.reject(new Error("native close failed"));
    },
  };
  const controller = new MidnightRuntimeController({ api, transport });
  await controller.openWalletSession(config, secrets);
  await controller.openWalletSession(
    { ...config, walletFingerprint: "second" },
    secrets,
  );
  await assert.rejects(controller.dispose(), { code: "NATIVE_INTERNAL" });
});
