import { decodeBase64, encodeBase64 } from "./base64.js";
import type {
  MidnightCommandKind,
  MidnightCommand,
  MidnightCommandResult,
} from "./commands.js";
import { MidnightRuntimeError } from "./errors.js";
import { silentMidnightLogger, type MidnightLogger } from "./host.js";
import type {
  MidnightEndpointRole,
  MidnightNetworkResult,
  MidnightOperationStep,
  MidnightRuntimeApi,
  MidnightSessionHandle,
} from "./runtime-types.js";

export interface MidnightFetchRequest {
  readonly method: "POST";
  readonly headers: Readonly<Record<string, string>>;
  readonly body: Uint8Array;
  readonly signal: AbortSignal;
}

export interface MidnightFetchResponse {
  readonly status: number;
  arrayBuffer(): Promise<ArrayBuffer>;
}

export type MidnightFetch = (
  url: string,
  request: MidnightFetchRequest,
) => Promise<MidnightFetchResponse>;

export interface MidnightWebSocket {
  readonly readyState: number;
  send(data: string | Uint8Array): void;
  close(code?: number, reason?: string): void;
  addEventListener(
    type: "message",
    listener: (event: { readonly data: string | ArrayBuffer }) => void,
  ): void;
  addEventListener(
    type: "open" | "error" | "close",
    listener: () => void,
  ): void;
}

export type MidnightWebSocketFactory = (url: string) => MidnightWebSocket;

export interface MidnightNetworkConfiguration {
  readonly indexerHttpUrl: string;
  readonly indexerWebSocketUrl: string;
  readonly proofServerUrl: string;
  readonly nodeUrl: string;
}

export interface MidnightTransportConfiguration {
  readonly network: MidnightNetworkConfiguration;
  readonly fetch: MidnightFetch;
  readonly createWebSocket: MidnightWebSocketFactory;
  readonly headers?: (
    role: MidnightEndpointRole,
  ) => Promise<Readonly<Record<string, string>>>;
  readonly timeoutMs?: number;
  readonly maximumEffectSteps?: number;
  readonly logger?: MidnightLogger;
}

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

export interface InternalMidnightProofAdapter {
  execute(effect: "check" | "prove", request: Uint8Array): Promise<Uint8Array>;
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

function endpoint(
  network: MidnightNetworkConfiguration,
  role: MidnightEndpointRole,
): string {
  if (role === "indexer") return network.indexerHttpUrl;
  if (role === "proof") return network.proofServerUrl;
  return network.nodeUrl;
}

function elapsed(startedAt: number): number {
  return Math.max(0, performance.now() - startedAt);
}

function positiveInteger(value: number | undefined, fallback: number): number {
  const resolved = value ?? fallback;
  if (!Number.isSafeInteger(resolved) || resolved <= 0) {
    throw new MidnightRuntimeError("INVALID_ARGUMENT");
  }
  return resolved;
}

function requireNotAborted(signal: AbortSignal | undefined): void {
  if (signal?.aborted === true) {
    throw new MidnightRuntimeError("CANCELLED");
  }
}

async function readBody(response: MidnightFetchResponse): Promise<Uint8Array> {
  return new Uint8Array(await response.arrayBuffer());
}

function acceptedResult(
  step: NetworkStep<MidnightCommandKind>,
  outcome: MidnightNetworkResult["outcome"],
  bytes: Uint8Array,
): MidnightNetworkResult {
  return {
    effectId: step.effectId,
    outcome,
    ...(bytes.length === 0 ? {} : { bodyBase64: encodeBase64(bytes) }),
  };
}

interface LinkedAbortController {
  readonly controller: AbortController;
  release(): void;
}

function createLinkedAbortController(
  signal: AbortSignal | undefined,
  timeoutMs: number,
): LinkedAbortController {
  const controller = new AbortController();
  const abort = () => {
    controller.abort();
  };
  const timeout = setTimeout(abort, timeoutMs);
  signal?.addEventListener("abort", abort, { once: true });
  if (signal?.aborted === true) controller.abort();
  return {
    controller,
    release() {
      clearTimeout(timeout);
      signal?.removeEventListener("abort", abort);
    },
  };
}

function responseOutcome(status: number): MidnightNetworkResult["outcome"] {
  if (status >= 200 && status < 300) return "accepted";
  if (status >= 400 && status < 500) return "rejected";
  return "statusUnknown";
}

function logResponse(
  logger: MidnightLogger,
  step: NetworkStep<MidnightCommandKind>,
  outcome: MidnightNetworkResult["outcome"],
  startedAt: number,
  responseBytes: Uint8Array,
): void {
  logger.write({
    level: outcome === "statusUnknown" ? "warn" : "debug",
    event: "transport.response",
    role: step.endpointRole,
    effect: step.effect,
    outcome,
    durationMs: elapsed(startedAt),
    byteLength: responseBytes.length,
  });
}

async function requestNetwork(
  config: MidnightTransportConfiguration,
  step: NetworkStep<MidnightCommandKind>,
  body: Uint8Array,
  signal: AbortSignal,
  logger: MidnightLogger,
  startedAt: number,
): Promise<MidnightNetworkResult> {
  const extraHeaders = (await config.headers?.(step.endpointRole)) ?? {};
  const response = await config.fetch(
    endpoint(config.network, step.endpointRole),
    {
      method: "POST",
      headers: {
        "content-type": "application/octet-stream",
        ...extraHeaders,
      },
      body,
      signal,
    },
  );
  const responseBytes = await readBody(response);
  const outcome = responseOutcome(response.status);
  logResponse(logger, step, outcome, startedAt, responseBytes);
  if (outcome === "statusUnknown" && step.effect !== "submit") {
    throw new MidnightRuntimeError("TRANSPORT_ERROR");
  }
  return acceptedResult(step, outcome, responseBytes);
}

function recoverNetworkFailure(
  error: unknown,
  logger: MidnightLogger,
  step: NetworkStep<MidnightCommandKind>,
  signal: AbortSignal | undefined,
  startedAt: number,
): MidnightNetworkResult {
  if (signal?.aborted === true) {
    throw new MidnightRuntimeError("CANCELLED");
  }
  logger.write({
    level: "warn",
    event: "transport.failure",
    role: step.endpointRole,
    effect: step.effect,
    durationMs: elapsed(startedAt),
    errorCode:
      step.effect === "submit"
        ? "SUBMISSION_STATUS_UNKNOWN"
        : "TRANSPORT_ERROR",
  });
  if (step.effect === "submit") {
    return acceptedResult(step, "statusUnknown", new Uint8Array());
  }
  if (error instanceof MidnightRuntimeError) throw error;
  throw new MidnightRuntimeError("TRANSPORT_ERROR");
}

async function executeNetwork(
  config: MidnightTransportConfiguration,
  step: NetworkStep<MidnightCommandKind>,
  signal: AbortSignal | undefined,
  timeoutMs: number,
  proofAdapter?: InternalMidnightProofAdapter,
): Promise<MidnightNetworkResult> {
  const logger = config.logger ?? silentMidnightLogger;
  const bytes = decodeBase64(step.bodyBase64);
  const startedAt = performance.now();
  const linked = createLinkedAbortController(signal, timeoutMs);
  logger.write({
    level: "debug",
    event: "transport.request",
    role: step.endpointRole,
    effect: step.effect,
    byteLength: bytes.length,
  });
  try {
    if (
      proofAdapter !== undefined &&
      step.endpointRole === "proof" &&
      (step.effect === "check" || step.effect === "prove")
    ) {
      const responseBytes = await proofAdapter.execute(step.effect, bytes);
      logResponse(logger, step, "accepted", startedAt, responseBytes);
      return acceptedResult(step, "accepted", responseBytes);
    }
    return await requestNetwork(
      config,
      step,
      bytes,
      linked.controller.signal,
      logger,
      startedAt,
    );
  } catch (error) {
    return recoverNetworkFailure(error, logger, step, signal, startedAt);
  } finally {
    linked.release();
    bytes.fill(0);
  }
}

async function notifyStep<K extends MidnightCommandKind>(
  options: MidnightRunCommandOptions<K> | undefined,
  step: MidnightOperationStep<K>,
): Promise<void> {
  await options?.onStep?.(step);
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
  return {
    async runCommand(api, session, command, options) {
      requireNotAborted(options?.signal);
      let step = await api.beginCommand(session, command);
      let operation = step.kind === "complete" ? null : step.operation;
      try {
        for (let count = 0; count < maximumEffectSteps; count += 1) {
          requireNotAborted(options?.signal);
          await notifyStep(options, step);
          requireNotAborted(options?.signal);
          if (step.kind === "complete") return step.result;
          operation = step.operation;
          const result =
            step.kind === "network"
              ? await executeNetwork(
                  normalized,
                  step,
                  options?.signal,
                  timeoutMs,
                  proofAdapter,
                )
              : null;
          step = await api.resumeOperation(step.operation, result);
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
