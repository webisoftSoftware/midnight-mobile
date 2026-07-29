import {
  MidnightRuntimeError,
  type MidnightCommandKind,
  type MidnightOperationStep,
  type MidnightRuntimeController,
  type MidnightRuntimeStatus,
  type MidnightWalletSessionSecrets,
} from "@1am/midnight-mobile";

import type { MockExampleRuntime } from "./example-runtime";
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
  readonly cancellationError: "CANCELLED";
  readonly recoverableError: "TRANSPORT_ERROR";
}

const wallet = {
  networkId: "preview",
  walletFingerprint: "synthetic-mock-wallet",
  unshieldedAddress: "mn_addr_preview1synthetic",
} as const;

function syntheticMockKeyMaterial(): MidnightWalletSessionSecrets {
  return {
    nightExternalKey: new Uint8Array(32),
    zswapSeed: new Uint8Array(32),
    dustSeed: new Uint8Array(32),
  };
}

function wipe(values: MidnightWalletSessionSecrets): void {
  values.nightExternalKey.fill(0);
  values.zswapSeed.fill(0);
  values.dustSeed.fill(0);
}

async function captureError<T extends "CANCELLED" | "TRANSPORT_ERROR">(
  operation: Promise<unknown>,
  expected: T,
): Promise<T> {
  try {
    await operation;
  } catch (error) {
    if (error instanceof MidnightRuntimeError && error.code === expected) {
      return expected;
    }
    throw error;
  }
  throw new Error(`expected ${expected}`);
}

async function openWithSyntheticSecrets(controller: MidnightRuntimeController) {
  const secrets = syntheticMockKeyMaterial();
  try {
    return await controller.openWalletSession(wallet, secrets);
  } finally {
    wipe(secrets);
  }
}

async function synchronize(
  controller: MidnightRuntimeController,
): Promise<void> {
  const session = await openWithSyntheticSecrets(controller);
  for (const stream of ["shielded", "unshielded", "dust"] as const) {
    await controller.applySyncBatch(session, {
      stream,
      fromOffset: 0,
      toOffset: 1,
      payloads: [Uint8Array.of(1)],
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
  environment: MockExampleRuntime,
): Promise<{
  readonly transactionHash: string;
  readonly submissionStatus: "accepted";
  readonly operationSteps: readonly string[];
  readonly cancellationError: "CANCELLED";
  readonly recoverableError: "TRANSPORT_ERROR";
}> {
  try {
    const session = await openWithSyntheticSecrets(controller);
    const operationSteps: string[] = [];
    const transfer = await controller.runCommand(
      session,
      {
        kind: "transfer",
        to: "mn_shield-addr_preview1synthetic",
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
    const aborter = new AbortController();
    const cancellation = controller.runCommand(
      session,
      { kind: "createProvingPayload", preimageBase64: "AA==" },
      { signal: aborter.signal },
    );
    aborter.abort();
    const cancellationError = await captureError(cancellation, "CANCELLED");
    environment.services.failNextProofRequest();
    const recoverableError = await captureError(
      controller.runCommand(session, {
        kind: "createProvingPayload",
        preimageBase64: "AA==",
      }),
      "TRANSPORT_ERROR",
    );
    await controller.closeWalletSession(session);
    if (submission.status !== "accepted") {
      throw new Error("mock submission was rejected");
    }
    return {
      transactionHash: transfer.transactionHash,
      submissionStatus: submission.status,
      operationSteps,
      cancellationError,
      recoverableError,
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
      "preview:synthetic-mock-wallet",
    );
    if (checkpoint?.bytes.length !== 3) {
      throw new Error("mock checkpoint was not persisted");
    }
    restored = environment.createRestoredController();
    const restoredSession = await openWithSyntheticSecrets(restored);
    const snapshot = await restored.getWalletSnapshot(restoredSession);
    await restored.closeWalletSession(restoredSession);
    const commandResults = await exerciseCommands(
      environment.createRestoredController(),
      environment,
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
