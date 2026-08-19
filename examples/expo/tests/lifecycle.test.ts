import assert from "node:assert/strict";
import test from "node:test";

import { createMockExampleRuntime } from "../src/example-runtime";
import { runMockedWalletLifecycle } from "../src/lifecycle";
import type { ExamplePlatform } from "../src/mock-services";

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
  });
}
