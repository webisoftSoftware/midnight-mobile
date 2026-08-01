import assert from "node:assert/strict";
import test from "node:test";

import {
  createLivePreviewRuntime,
  createMockExampleRuntime,
} from "../src/example-runtime";
import { runMockedWalletLifecycle } from "../src/lifecycle";
import {
  MockMidnightServices,
  type ExamplePlatform,
} from "../src/mock-services";

for (const platform of [
  "ios",
  "android",
] as const satisfies readonly ExamplePlatform[]) {
  await test(`mocked wallet lifecycle completes on ${platform}`, async () => {
    const report = await runMockedWalletLifecycle(
      createMockExampleRuntime(platform),
    );

    assert.equal(report.platform, platform);
    assert.equal(report.finalStatus, "ready");
    assert.equal(report.totalShielded, "120");
    assert.equal(report.totalUnshielded, "30");
    assert.equal(report.dustBalance, "9");
    assert.equal(report.checkpointRestored, true);
    assert.deepEqual(Array.from(new Set(report.runtimeStatuses)), [
      "opening",
      "syncing",
      "ready",
      "closing",
      "idle",
    ]);
    assert.match(report.transactionHash, /^[0-9a-f]{64}$/);
    assert.equal(report.submissionStatus, "accepted");
    assert.deepEqual(report.operationSteps, [
      "progress",
      "network:proof:prove",
      "network:proof:balance",
      "complete",
      "network:node:submit",
      "complete",
    ]);
    assert.equal(report.cancellationError, "CANCELLED");
    assert.equal(report.recoverableError, "TRANSPORT_ERROR");
  });
}

await test("explicit live open consumes and wipes caller key buffers", async () => {
  const services = new MockMidnightServices();
  const secrets = {
    nightExternalKey: new Uint8Array(32).fill(7),
    zswapSeed: new Uint8Array(32).fill(8),
    dustSeed: new Uint8Array(32).fill(9),
  };
  const live = createLivePreviewRuntime({
    mode: "live",
    wallet: {
      networkId: "preview",
      walletFingerprint: "synthetic-live-contract-test",
      unshieldedAddress: "mn_addr_preview1synthetic",
    },
    secrets,
    indexerHttpUrl: "https://indexer.caller.invalid/request",
    indexerWebSocketUrl: "wss://indexer.caller.invalid/stream",
    proofServerUrl: "https://proof.caller.invalid/request",
    nodeUrl: "https://node.caller.invalid/request",
    fetch: services.fetch,
    createWebSocket: () => services.createWebSocket(),
    endpointHeaders: () => Promise.resolve({}),
  });

  await assert.rejects(live.openWalletSession(), { code: "UNAVAILABLE" });
  assert.deepEqual(secrets.nightExternalKey, new Uint8Array(32));
  assert.deepEqual(secrets.zswapSeed, new Uint8Array(32));
  assert.deepEqual(secrets.dustSeed, new Uint8Array(32));
  await live.controller.dispose();
});
