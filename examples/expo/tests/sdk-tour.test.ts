import assert from "node:assert/strict";
import test from "node:test";

import {
  createDemoWalletFixture,
  DEMO_RECIPIENT_ADDRESS,
  DEMO_WALLET,
} from "../src/demo-wallet";
import { createMockExampleRuntime } from "../src/example-runtime";
import { runSdkExample } from "../src/sdk-tour";
import type { NativeSmokeReport } from "../src/native-smoke";

await test("public demo wallet returns fresh wipeable key buffers", () => {
  const first = createDemoWalletFixture();
  const second = createDemoWalletFixture();

  assert.equal(
    first.wallet.unshieldedAddress,
    "mn_addr_preview1xnd9uwtfx4mecjyrtflkpz6kwy4m4v90jspddjpqvqef2v46de9sk7ulc4",
  );
  assert.equal(first.wallet.unshieldedAddress, DEMO_WALLET.unshieldedAddress);
  assert.equal(
    DEMO_RECIPIENT_ADDRESS,
    "mn_shield-addr_preview1k5f2raelljee2kzz5crzwtg70veyqh4l2w33ycjytay5z9eaw5l2ftj36j6jhegppwa4y3pdpa87lv343rt9ez5pxkfr0q7fp2zw22qxf08s0",
  );
  assert.notStrictEqual(
    first.secrets.nightExternalKey,
    second.secrets.nightExternalKey,
  );
  assert.notStrictEqual(first.secrets.zswapSeed, second.secrets.zswapSeed);
  assert.notStrictEqual(first.secrets.dustSeed, second.secrets.dustSeed);
  first.secrets.nightExternalKey.fill(0);
  assert.equal(second.secrets.nightExternalKey[0], 0x11);
});

const passingNativeReport: NativeSmokeReport = {
  passed: true,
  steps: [{ name: "complete", ok: true, detail: "ok" }],
};

await test("SDK example combines host, native, and prover phases", async () => {
  const environment = createMockExampleRuntime("android");
  const report = await runSdkExample(
    environment,
    "android",
    environment.createController(),
    {
      runNativeSmoke: () => Promise.resolve(passingNativeReport),
      runLocalProver: () => Promise.resolve(passingNativeReport),
    },
  );

  assert.equal(report.passed, true);
  assert.equal(report.walletAddress, DEMO_WALLET.unshieldedAddress);
  assert.deepEqual(
    report.steps.map((step) => step.id),
    ["mock-host", "native-runtime", "native-prover"],
  );
  assert.ok(report.steps.every((step) => step.passed));
  assert.match(report.steps[0]?.detail ?? "", /checkpoint restored/);
});

await test("SDK example reports native phase failures without rejecting", async () => {
  const environment = createMockExampleRuntime("ios");
  const report = await runSdkExample(
    environment,
    "ios",
    environment.createController(),
    {
      runNativeSmoke: () => Promise.reject(new Error("synthetic failure")),
      runLocalProver: () => Promise.reject(new Error("synthetic failure")),
    },
  );

  assert.equal(report.passed, false);
  assert.deepEqual(
    report.steps.map((step) => step.passed),
    [true, false, false],
  );
  assert.equal(report.steps[1]?.detail, "UNEXPECTED_EXAMPLE_FAILURE");
  assert.equal(report.steps[2]?.detail, "UNEXPECTED_EXAMPLE_FAILURE");
});
