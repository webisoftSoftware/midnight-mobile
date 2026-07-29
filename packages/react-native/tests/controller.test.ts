import assert from "node:assert/strict";
import test from "node:test";

import type {
  MidnightCommand,
  MidnightCommandKind,
  MidnightCommandResult,
} from "../src/commands.js";
import { MidnightRuntimeController } from "../src/controller.js";
import { InMemoryMidnightCheckpointStore } from "../src/host.js";
import type {
  MidnightOperationStep,
  MidnightRuntimeApi,
  MidnightWalletSnapshot,
} from "../src/runtime-types.js";
import type { MidnightStandardTransport } from "../src/transport.js";

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
  const config = {
    networkId: "preprod",
    walletFingerprint: "synthetic-wallet",
    unshieldedAddress: "mn_addr_preprod1synthetic",
  } as const;
  const session = await controller.openWalletSession(config, {
    nightExternalKey: new Uint8Array(32),
    zswapSeed: new Uint8Array(32),
    dustSeed: new Uint8Array(32),
  });
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
