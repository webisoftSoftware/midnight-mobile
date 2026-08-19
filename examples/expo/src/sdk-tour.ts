import {
  MidnightRuntimeError,
  type MidnightRuntimeController,
} from "@1am/midnight-mobile";
import { MidnightLocalProverError } from "@1am/midnight-mobile/local-prover";

import type { MockExampleRuntime } from "./example-runtime";
import { DEMO_WALLET } from "./demo-wallet";
import { runMockedWalletLifecycle } from "./lifecycle";
import { runLocalProverSmokeTest } from "./local-prover-demo";
import type { ExamplePlatform } from "./mock-services";
import { runNativeSmokeTest, type NativeSmokeReport } from "./native-smoke";

export interface SdkExampleStep {
  readonly id: "mock-host" | "native-runtime" | "native-prover";
  readonly label: string;
  readonly detail: string;
  readonly passed: boolean;
}

export interface SdkExampleReport {
  readonly passed: boolean;
  readonly walletAddress: string;
  readonly steps: readonly SdkExampleStep[];
}

export interface SdkExampleDependencies {
  readonly runNativeSmoke?: () => Promise<NativeSmokeReport>;
  readonly runLocalProver?: (
    platform: ExamplePlatform,
  ) => Promise<NativeSmokeReport>;
}

function failureCode(failure: unknown): string {
  if (
    failure instanceof MidnightRuntimeError ||
    failure instanceof MidnightLocalProverError
  ) {
    return failure.code;
  }
  return "UNEXPECTED_EXAMPLE_FAILURE";
}

function nativeFailureDetail(report: NativeSmokeReport): string {
  const detail = report.steps
    .filter((step) => !step.ok)
    .map((step) => `${step.name}: ${step.detail}`)
    .join(" · ");
  return detail.length === 0 ? "NATIVE_INTERNAL" : detail;
}

async function runNativePhase(
  id: "native-runtime" | "native-prover",
  label: string,
  successDetail: string,
  run: () => Promise<NativeSmokeReport>,
): Promise<SdkExampleStep> {
  try {
    const report = await run();
    return {
      id,
      label,
      passed: report.passed,
      detail: report.passed ? successDetail : nativeFailureDetail(report),
    };
  } catch (failure: unknown) {
    return { id, label, passed: false, detail: failureCode(failure) };
  }
}

/**
 * Runs the complete example in one guided flow:
 *
 * - the mock-host phase exercises the controller, sync, commands, checkpoint,
 *   cancellation, and recoverable transport paths;
 * - the native-runtime phase loads the packaged Rust runtime and repeats the
 *   wallet/session primitives on the device; and
 * - the native-prover phase runs real check and prove calls against bundled
 *   public spend-circuit artifacts.
 */
export async function runSdkExample(
  environment: MockExampleRuntime,
  platform: ExamplePlatform,
  controller: MidnightRuntimeController,
  dependencies: SdkExampleDependencies = {},
): Promise<SdkExampleReport> {
  const steps: SdkExampleStep[] = [];

  try {
    const report = await runMockedWalletLifecycle(environment, controller);
    steps.push({
      id: "mock-host",
      label: "Mock host flow",
      passed: true,
      detail: [
        `sync ${report.totalShielded} shielded / ${report.totalUnshielded} unshielded`,
        `dust ${report.dustBalance}`,
        `transaction ${report.submissionStatus}`,
        "checkpoint restored",
      ].join(" · "),
    });
  } catch (failure: unknown) {
    steps.push({
      id: "mock-host",
      label: "Mock host flow",
      passed: false,
      detail: failureCode(failure),
    });
  }

  steps.push(
    await runNativePhase(
      "native-runtime",
      "Native runtime",
      "session opened and closed · snapshot read · data signed · checkpoint restored",
      dependencies.runNativeSmoke ?? runNativeSmokeTest,
    ),
  );
  steps.push(
    await runNativePhase(
      "native-prover",
      "Native prover",
      "artifacts verified · check passed · proof created",
      () => (dependencies.runLocalProver ?? runLocalProverSmokeTest)(platform),
    ),
  );

  return {
    passed: steps.every((step) => step.passed),
    walletAddress: DEMO_WALLET.unshieldedAddress,
    steps,
  };
}
