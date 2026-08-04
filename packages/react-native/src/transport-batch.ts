import { decodeBase64, encodeBase64 } from "./base64.js";
import { MidnightRuntimeError } from "./errors.js";
import { silentMidnightLogger, type MidnightLogger } from "./host.js";
import type {
  MidnightEndpointRole,
  MidnightNetworkEffect,
  MidnightNetworkResult,
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
  /**
   * Upper bound on proof effects executed concurrently within a single
   * batched round. Only proof (`check`/`prove`) effects are ever run
   * concurrently; any other batch content runs sequentially regardless of
   * this value. Default 2, maximum 8.
   */
  readonly maximumConcurrentProofs?: number;
  readonly logger?: MidnightLogger;
}

export interface InternalMidnightProofAdapter {
  execute(effect: "check" | "prove", request: Uint8Array): Promise<Uint8Array>;
  /**
   * Optional native fast path for a `prove`-only batch: one call proves every
   * request and returns responses in the same order. Absent on native
   * modules that only implement single-request `prove`; callers must fall
   * back to individually executing each request under a concurrency limit.
   */
  executeProveBatch?(
    requests: readonly Uint8Array[],
  ): Promise<readonly Uint8Array[]>;
}

/**
 * The shape common to both a singular network step and a single element of
 * its `effects` batch. Every helper below only ever needs these four fields,
 * so they accept this narrower shape and work unchanged for either source.
 */
export interface NetworkEffectRequest {
  readonly effectId: string;
  readonly effect: MidnightNetworkEffect;
  readonly endpointRole: MidnightEndpointRole;
  readonly bodyBase64: string;
}

function endpoint(
  network: MidnightNetworkConfiguration,
  role: MidnightEndpointRole,
  effect: MidnightNetworkEffect,
): string {
  if (role === "indexer") return network.indexerHttpUrl;
  if (role === "proof") {
    const route =
      effect === "check" || effect === "prove"
        ? effect
        : effect === "proveAndBalance"
          ? "prove-and-balance"
          : effect === "balance"
            ? "balance-only"
            : undefined;
    if (route !== undefined) {
      const url = new URL(network.proofServerUrl);
      url.pathname = `${url.pathname.replace(/\/+$/u, "")}/${route}`;
      return url.toString();
    }
    return network.proofServerUrl;
  }
  return network.nodeUrl;
}

function elapsed(startedAt: number): number {
  return Math.max(0, performance.now() - startedAt);
}

export function isProofEffect(effect: NetworkEffectRequest): boolean {
  return (
    effect.endpointRole === "proof" &&
    (effect.effect === "check" || effect.effect === "prove")
  );
}

export function requireNotAborted(signal: AbortSignal | undefined): void {
  if (signal?.aborted === true) {
    throw new MidnightRuntimeError("CANCELLED");
  }
}

async function readBody(response: MidnightFetchResponse): Promise<Uint8Array> {
  return new Uint8Array(await response.arrayBuffer());
}

function acceptedResult(
  step: NetworkEffectRequest,
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
  step: NetworkEffectRequest,
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
  step: NetworkEffectRequest,
  body: Uint8Array,
  signal: AbortSignal,
  logger: MidnightLogger,
  startedAt: number,
): Promise<MidnightNetworkResult> {
  const extraHeaders = (await config.headers?.(step.endpointRole)) ?? {};
  const response = await config.fetch(
    endpoint(config.network, step.endpointRole, step.effect),
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
  step: NetworkEffectRequest,
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

export async function executeNetwork(
  config: MidnightTransportConfiguration,
  step: NetworkEffectRequest,
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

/**
 * Executes a batch of independent effects under a bounded-concurrency
 * limiter: at most `limit` calls to `executeNetwork` are ever in flight at
 * once. Results are written to their original index, so ordering is
 * deterministic even though execution interleaves.
 *
 * Cancellation contract: the signal is checked before the batch starts and
 * before each new admission. Once it fires, no new work is admitted, but
 * every call already in flight is awaited to completion (via `Promise.all`
 * over the worker loops) before `CANCELLED` is thrown — nothing is left
 * dangling. The same discipline applies to a genuine execution error: the
 * first one stops further admission, and it is rethrown only after every
 * in-flight call has settled.
 */
export async function executeConcurrencyLimited(
  config: MidnightTransportConfiguration,
  effects: readonly NetworkEffectRequest[],
  signal: AbortSignal | undefined,
  timeoutMs: number,
  proofAdapter: InternalMidnightProofAdapter | undefined,
  limit: number,
): Promise<MidnightNetworkResult[]> {
  const results: MidnightNetworkResult[] = new Array<MidnightNetworkResult>(
    effects.length,
  );
  let nextIndex = 0;
  let stopAdmission = false;
  let firstError: Error | undefined;

  async function worker(): Promise<void> {
    for (;;) {
      if (stopAdmission || signal?.aborted === true) {
        stopAdmission = true;
        return;
      }
      const index = nextIndex;
      const effect = effects[index];
      if (effect === undefined) return;
      nextIndex += 1;
      try {
        results[index] = await executeNetwork(
          config,
          effect,
          signal,
          timeoutMs,
          proofAdapter,
        );
      } catch (error) {
        stopAdmission = true;
        firstError ??=
          error instanceof Error ? error : new Error(String(error));
      }
    }
  }

  const workerCount = Math.max(1, Math.min(limit, effects.length));
  await Promise.all(Array.from({ length: workerCount }, () => worker()));

  if (signal?.aborted === true) {
    throw new MidnightRuntimeError("CANCELLED");
  }
  if (firstError !== undefined) {
    throw firstError;
  }
  return results;
}

/**
 * Fast path for a `prove`-only batch when the proof adapter exposes a native
 * batch entrypoint: one call proves every request. Falls back semantics are
 * the caller's responsibility (see `executeBatch`); this function assumes
 * `proofAdapter.executeProveBatch` is defined.
 */
export async function executeProveOnlyBatch(
  config: MidnightTransportConfiguration,
  effects: readonly NetworkEffectRequest[],
  proofAdapter: InternalMidnightProofAdapter,
  signal: AbortSignal | undefined,
): Promise<MidnightNetworkResult[]> {
  const logger = config.logger ?? silentMidnightLogger;
  const bodies = effects.map((effect) => decodeBase64(effect.bodyBase64));
  const startedAt = performance.now();
  try {
    requireNotAborted(signal);
    const responses = await proofAdapter.executeProveBatch?.(bodies);
    if (responses?.length !== effects.length) {
      throw new MidnightRuntimeError("NATIVE_INTERNAL");
    }
    return effects.map((effect, index) => {
      const responseBytes = responses[index];
      // A hole or an empty proof must fail the round here. Letting it through
      // would emit an "accepted" result with no body, which the runtime can only
      // report as a generic proof failure well after the useful context is gone.
      if (responseBytes === undefined || responseBytes.length === 0) {
        throw new MidnightRuntimeError("NATIVE_INTERNAL");
      }
      logResponse(logger, effect, "accepted", startedAt, responseBytes);
      return acceptedResult(effect, "accepted", responseBytes);
    });
  } catch (error) {
    if (signal?.aborted === true) {
      throw new MidnightRuntimeError("CANCELLED");
    }
    if (error instanceof MidnightRuntimeError) throw error;
    throw new MidnightRuntimeError("TRANSPORT_ERROR");
  } finally {
    for (const body of bodies) body.fill(0);
  }
}

/**
 * Executes every effect in a batched round. A batch that is entirely
 * `prove` effects goes through the native batch entrypoint when the proof
 * adapter offers one; a batch that is entirely proof effects (`check` and/or
 * `prove`) otherwise runs under the bounded concurrency limiter; any other
 * batch content (fail-safe: mixed with non-proof effects) runs strictly
 * sequentially (limit of 1), matching today's serial indexer/node behaviour.
 */
export async function executeBatch(
  config: MidnightTransportConfiguration,
  effects: readonly NetworkEffectRequest[],
  signal: AbortSignal | undefined,
  timeoutMs: number,
  proofAdapter: InternalMidnightProofAdapter | undefined,
  concurrencyLimit: number,
): Promise<MidnightNetworkResult[]> {
  requireNotAborted(signal);
  const allProofEffects = effects.every(isProofEffect);
  const allProveOnly =
    allProofEffects && effects.every((effect) => effect.effect === "prove");
  if (allProveOnly && proofAdapter?.executeProveBatch !== undefined) {
    return executeProveOnlyBatch(config, effects, proofAdapter, signal);
  }
  const limit = allProofEffects
    ? Math.max(1, Math.min(concurrencyLimit, effects.length))
    : 1;
  return executeConcurrencyLimited(
    config,
    effects,
    signal,
    timeoutMs,
    proofAdapter,
    limit,
  );
}
