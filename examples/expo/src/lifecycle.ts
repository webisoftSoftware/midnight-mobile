import {
  type MidnightCommandKind,
  type MidnightOperationStep,
  type MidnightRuntimeController,
  type MidnightRuntimeStatus,
  type MidnightWalletSessionSecrets,
} from "@1am/midnight-mobile";

import type { MockExampleRuntime } from "./example-runtime";
import { createDemoWalletFixture, DEMO_RECIPIENT_ADDRESS } from "./demo-wallet";
import type { ExamplePlatform } from "./mock-services";

export interface MockLifecycleReport {
  readonly platform: ExamplePlatform;
  readonly finalStatus: "ready";
  readonly totalShielded: string;
  readonly totalUnshielded: string;
  readonly dustBalance: string;
  readonly checkpointRestored: true;
  readonly transactionHash: string;
  readonly submissionStatus: "accepted";
  readonly runtimeStatuses: readonly MidnightRuntimeStatus[];
  readonly operationSteps: readonly string[];
}

function wipe(values: MidnightWalletSessionSecrets): void {
  values.nightExternalKey.fill(0);
  values.zswapSeed.fill(0);
  values.dustSeed.fill(0);
}

async function openDemoWallet(controller: MidnightRuntimeController) {
  const { wallet, secrets } = createDemoWalletFixture();
  try {
    return await controller.openWalletSession(wallet, secrets);
  } finally {
    wipe(secrets);
  }
}

async function synchronize(
  controller: MidnightRuntimeController,
): Promise<void> {
  const session = await openDemoWallet(controller);
  for (const stream of ["shielded", "unshielded", "dust"] as const) {
    await controller.applySyncBatch(session, {
      stream,
      fromOffset: 0,
      toOffset: 1,
      payloads: [Uint8Array.of(1)],
    });
    await controller.applySyncBatch(session, {
      stream,
      fromOffset: 1,
      toOffset: 1,
      payloads: [],
    });
  }
  await controller.closeWalletSession(session);
}

function recordStep<K extends MidnightCommandKind>(
  steps: string[],
  step: MidnightOperationStep<K>,
): void {
  steps.push(
    step.kind === "network"
      ? `${step.kind}:${step.endpointRole}:${step.effect}`
      : step.kind,
  );
}

async function exerciseCommands(
  controller: MidnightRuntimeController,
): Promise<{
  readonly transactionHash: string;
  readonly submissionStatus: "accepted";
  readonly operationSteps: readonly string[];
}> {
  try {
    const session = await openDemoWallet(controller);
    const operationSteps: string[] = [];
    const transfer = await controller.runCommand(
      session,
      {
        kind: "transfer",
        to: DEMO_RECIPIENT_ADDRESS,
        amount: "5",
        tokenType: "66".repeat(32),
        walletType: "shielded",
      },
      {
        onStep: (step) => {
          recordStep(operationSteps, step);
        },
      },
    );
    const submission = await controller.runCommand(
      session,
      { kind: "submitFinalized", rawBase64: transfer.transactionBase64 },
      {
        onStep: (step) => {
          recordStep(operationSteps, step);
        },
      },
    );
    await controller.closeWalletSession(session);
    if (submission.status !== "accepted") {
      throw new Error("mock submission was rejected");
    }
    return {
      transactionHash: transfer.transactionHash,
      submissionStatus: submission.status,
      operationSteps,
    };
  } finally {
    await controller.dispose();
  }
}

export async function runMockedWalletLifecycle(
  environment: MockExampleRuntime,
  controller = environment.createController(),
): Promise<MockLifecycleReport> {
  let restored: MidnightRuntimeController | null = null;
  const runtimeStatuses: MidnightRuntimeStatus[] = [];
  const unsubscribe = controller.subscribe((status) => {
    runtimeStatuses.push(status);
  });
  try {
    await synchronize(controller);
    const checkpoint = await environment.checkpointStore.load(
      "preview:public-example-wallet-v1",
    );
    if (checkpoint?.bytes.length !== 6) {
      throw new Error("mock checkpoint was not persisted");
    }
    restored = environment.createRestoredController();
    const restoredSession = await openDemoWallet(restored);
    const snapshot = await restored.getWalletSnapshot(restoredSession);
    await restored.closeWalletSession(restoredSession);
    const commandResults = await exerciseCommands(
      environment.createRestoredController(),
    );
    return {
      platform: environment.platform,
      finalStatus: snapshot.status === "ready" ? "ready" : neverReady(),
      totalShielded: snapshot.balances.totalShielded,
      totalUnshielded: snapshot.balances.totalUnshielded,
      dustBalance: snapshot.balances.dustBalance,
      checkpointRestored: true,
      runtimeStatuses,
      ...commandResults,
    };
  } finally {
    unsubscribe();
    await controller.dispose();
    await restored?.dispose();
  }
}

function neverReady(): never {
  throw new Error("restored mock wallet is not ready");
}
