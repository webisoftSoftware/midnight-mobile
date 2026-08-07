import { decodeBase64 } from "./base64.js";
import type {
  MidnightCommandKind,
  MidnightCommand,
  MidnightCommandResult,
} from "./commands.js";
import { MidnightRuntimeError } from "./errors.js";
import type {
  MidnightOperationStep,
  MidnightRuntimeApi,
  MidnightSessionHandle,
} from "./runtime-types.js";
import {
  executeBatch,
  executeNetwork,
  requireNotAborted,
  type InternalMidnightProofAdapter,
  type MidnightFetch,
  type MidnightFetchRequest,
  type MidnightFetchResponse,
  type MidnightNetworkConfiguration,
  type MidnightTransportConfiguration,
  type MidnightWebSocket,
  type MidnightWebSocketFactory,
} from "./transport-batch.js";

export type {
  InternalMidnightProofAdapter,
  MidnightFetch,
  MidnightFetchRequest,
  MidnightFetchResponse,
  MidnightNetworkConfiguration,
  MidnightTransportConfiguration,
  MidnightWebSocket,
  MidnightWebSocketFactory,
};

export interface MidnightRunCommandOptions<K extends MidnightCommandKind> {
  readonly signal?: AbortSignal;
  readonly onStep?: (step: MidnightOperationStep<K>) => void | Promise<void>;
}

type NetworkStep<K extends MidnightCommandKind> = Extract<
  MidnightOperationStep<K>,
  { readonly kind: "network" }
>;

export interface MidnightStandardTransport {
  runCommand<K extends MidnightCommandKind>(
    api: MidnightRuntimeApi,
    session: MidnightSessionHandle,
    command: MidnightCommand<K>,
    options?: MidnightRunCommandOptions<K>,
  ): Promise<MidnightCommandResult<K>>;
  openSyncSocket(
    onPayload: (payload: Uint8Array) => void,
    signal?: AbortSignal,
  ): MidnightWebSocket;
}

function checkedUrl(value: string, protocols: readonly string[]): string {
  let parsed: URL;
  try {
    parsed = new URL(value);
  } catch {
    throw new MidnightRuntimeError("INVALID_ARGUMENT");
  }
  if (!protocols.includes(parsed.protocol)) {
    throw new MidnightRuntimeError("INVALID_ARGUMENT");
  }
  return parsed.toString();
}

function positiveInteger(value: number | undefined, fallback: number): number {
  const resolved = value ?? fallback;
  if (!Number.isSafeInteger(resolved) || resolved <= 0) {
    throw new MidnightRuntimeError("INVALID_ARGUMENT");
  }
  return resolved;
}

function boundedPositiveInteger(
  value: number | undefined,
  fallback: number,
  max: number,
): number {
  const resolved = positiveInteger(value, fallback);
  if (resolved > max) {
    throw new MidnightRuntimeError("INVALID_ARGUMENT");
  }
  return resolved;
}

const MAXIMUM_CONCURRENT_PROOFS_CAP = 8;

async function notifyStep<K extends MidnightCommandKind>(
  options: MidnightRunCommandOptions<K> | undefined,
  step: MidnightOperationStep<K>,
): Promise<void> {
  await options?.onStep?.(step);
}

/**
 * Executes a single network round (single-effect or batched) and resumes
 * the operation with the corresponding result shape: a bare object for a
 * single effect (identical to the original behaviour), or an array when the
 * round carried a batch.
 */
async function advanceNetworkStep<K extends MidnightCommandKind>(
  api: MidnightRuntimeApi,
  config: MidnightTransportConfiguration & {
    network: MidnightNetworkConfiguration;
  },
  step: NetworkStep<K>,
  signal: AbortSignal | undefined,
  timeoutMs: number,
  proofAdapter: InternalMidnightProofAdapter | undefined,
  maximumConcurrentProofs: number,
): Promise<MidnightOperationStep<K>> {
  if (step.effects === undefined) {
    const result = await executeNetwork(
      config,
      step,
      signal,
      timeoutMs,
      proofAdapter,
    );
    return api.resumeOperation(step.operation, result);
  }
  const results = await executeBatch(
    config,
    step.effects,
    signal,
    timeoutMs,
    proofAdapter,
    maximumConcurrentProofs,
  );
  return api.resumeOperation(step.operation, results);
}

export function createStandardMidnightTransport(
  config: MidnightTransportConfiguration,
): MidnightStandardTransport {
  return createMidnightTransportWithProofAdapter(config);
}

export function createMidnightTransportWithProofAdapter(
  config: MidnightTransportConfiguration,
  proofAdapter?: InternalMidnightProofAdapter,
): MidnightStandardTransport {
  const network = {
    indexerHttpUrl: checkedUrl(config.network.indexerHttpUrl, [
      "http:",
      "https:",
    ]),
    indexerWebSocketUrl: checkedUrl(config.network.indexerWebSocketUrl, [
      "ws:",
      "wss:",
    ]),
    proofServerUrl: checkedUrl(config.network.proofServerUrl, [
      "http:",
      "https:",
    ]),
    nodeUrl: checkedUrl(config.network.nodeUrl, ["http:", "https:"]),
  };
  const normalized = { ...config, network };
  const timeoutMs = positiveInteger(config.timeoutMs, 30_000);
  const maximumEffectSteps = positiveInteger(config.maximumEffectSteps, 64);
  const maximumConcurrentProofs = boundedPositiveInteger(
    config.maximumConcurrentProofs,
    2,
    MAXIMUM_CONCURRENT_PROOFS_CAP,
  );
  return {
    async runCommand(api, session, command, options) {
      requireNotAborted(options?.signal);
      let step = await api.beginCommand(session, command);
      let operation = step.kind === "complete" ? null : step.operation;
      try {
        // `count` tracks rounds (one `resumeOperation` call each), not
        // individual effects, so a batched round consumes exactly one unit
        // of this cap regardless of how many effects it carries.
        for (let count = 0; count < maximumEffectSteps; count += 1) {
          requireNotAborted(options?.signal);
          await notifyStep(options, step);
          requireNotAborted(options?.signal);
          if (step.kind === "complete") return step.result;
          operation = step.operation;
          if (step.kind !== "network") {
            step = await api.resumeOperation(step.operation, null);
            continue;
          }
          step = await advanceNetworkStep(
            api,
            normalized,
            step,
            options?.signal,
            timeoutMs,
            proofAdapter,
            maximumConcurrentProofs,
          );
        }
        throw new MidnightRuntimeError("NATIVE_INTERNAL");
      } catch (error) {
        if (operation !== null) {
          await api.cancelOperation(operation).catch(() => undefined);
        }
        throw error;
      }
    },
    openSyncSocket(onPayload, signal) {
      const socket = normalized.createWebSocket(
        normalized.network.indexerWebSocketUrl,
      );
      socket.addEventListener("message", (event) => {
        onPayload(
          typeof event.data === "string"
            ? decodeBase64(event.data)
            : new Uint8Array(event.data),
        );
      });
      const close = () => {
        socket.close(1000, "cancelled");
      };
      signal?.addEventListener("abort", close, { once: true });
      return socket;
    },
  };
}
