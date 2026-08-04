import assert from "node:assert/strict";
import test from "node:test";

import { encodeBase64 } from "../src/base64.js";
import type { MidnightCommand, MidnightCommandKind } from "../src/commands.js";
import { decodeCommandResult } from "../src/decode.js";
import { silentMidnightLogger } from "../src/host.js";
import type {
  MidnightNetworkResult,
  MidnightOperationStep,
  MidnightRuntimeApi,
} from "../src/runtime-types.js";
import {
  createMidnightTransportWithProofAdapter,
  createStandardMidnightTransport,
  type InternalMidnightProofAdapter,
  type MidnightFetch,
  type MidnightWebSocket,
} from "../src/transport.js";

function idleApi(): MidnightRuntimeApi {
  return {
    openWalletSession() {
      return Promise.resolve({ id: 1, generation: 1 });
    },
    applySyncBatch() {
      return Promise.reject(new Error("unused"));
    },
    getWalletSnapshot() {
      return Promise.reject(new Error("unused"));
    },
    exportWalletCheckpoint() {
      return Promise.resolve({ version: 1, bytes: new Uint8Array() });
    },
    beginCommand() {
      return Promise.reject(new Error("unused"));
    },
    resumeOperation() {
      return Promise.reject(new Error("unused"));
    },
    cancelOperation() {
      return Promise.resolve();
    },
    closeWalletSession() {
      return Promise.resolve();
    },
  };
}

class RecordingSocket implements MidnightWebSocket {
  readonly readyState = 1;
  sent: string | Uint8Array | null = null;
  closed: readonly [number | undefined, string | undefined] | null = null;
  #messageListener:
    ((event: { readonly data: string | ArrayBuffer }) => void) | undefined;

  send(data: string | Uint8Array): void {
    this.sent = data;
  }

  close(code?: number, reason?: string): void {
    this.closed = [code, reason];
  }

  addEventListener(
    type: "message",
    listener: (event: { readonly data: string | ArrayBuffer }) => void,
  ): void;
  addEventListener(
    type: "open" | "error" | "close",
    listener: () => void,
  ): void;
  addEventListener(
    type: "message" | "open" | "error" | "close",
    listener:
      ((event: { readonly data: string | ArrayBuffer }) => void) | (() => void),
  ): void {
    if (type === "message") this.#messageListener = listener;
  }

  emit(data: string | ArrayBuffer): void {
    this.#messageListener?.({ data });
  }
}

function transportConfig(fetch: MidnightFetch) {
  return {
    network: {
      indexerHttpUrl: "https://indexer.example.invalid/request",
      indexerWebSocketUrl: "wss://indexer.example.invalid/stream",
      proofServerUrl: "https://proof.example.invalid/request",
      nodeUrl: "https://node.example.invalid/request",
    },
    fetch,
    createWebSocket() {
      return new RecordingSocket();
    },
    logger: silentMidnightLogger,
  } as const;
}

interface BatchEffectSpec {
  readonly id: string;
  readonly effect: "check" | "prove";
  readonly body: Uint8Array;
}

function batchNetworkStep<K extends MidnightCommandKind>(
  commandKind: K,
  effects: readonly BatchEffectSpec[],
): MidnightOperationStep<K> {
  const first = effects[0];
  if (first === undefined) throw new Error("effects must be non-empty");
  return {
    kind: "network",
    operation: { id: 9, generation: 1, commandKind },
    effectId: first.id,
    effect: first.effect,
    endpointRole: "proof",
    bodyBase64: encodeBase64(first.body),
    effects: effects.map((entry) => ({
      effectId: entry.id,
      effect: entry.effect,
      endpointRole: "proof",
      bodyBase64: encodeBase64(entry.body),
    })),
  };
}

/**
 * Builds a raw network step whose `effects` batch deliberately violates the
 * "proof effects only" wire contract, to exercise the fail-safe sequential
 * fallback. Real native code never emits this; the fallback exists purely
 * as a defence.
 */
function rawMixedBatchStep<K extends MidnightCommandKind>(
  commandKind: K,
  effects: readonly {
    readonly effectId: string;
    readonly effect: string;
    readonly endpointRole: string;
    readonly bodyBase64: string;
  }[],
): MidnightOperationStep<K> {
  const first = effects[0];
  if (first === undefined) throw new Error("effects must be non-empty");
  return {
    kind: "network",
    operation: { id: 9, generation: 1, commandKind },
    effectId: first.effectId,
    effect: first.effect,
    endpointRole: first.endpointRole,
    bodyBase64: first.bodyBase64,
    effects,
  } as unknown as MidnightOperationStep<K>;
}

function completingApi(
  beginStep: (
    commandKind: MidnightCommandKind,
  ) => MidnightOperationStep<MidnightCommandKind>,
  onResume: (
    result: MidnightNetworkResult | readonly MidnightNetworkResult[] | null,
  ) => void,
  onCancel?: () => void,
): MidnightRuntimeApi {
  return {
    ...idleApi(),
    beginCommand<K extends MidnightCommandKind>(
      _session: { readonly id: number; readonly generation: number },
      command: MidnightCommand<K>,
    ): Promise<MidnightOperationStep<K>> {
      return Promise.resolve(
        beginStep(command.kind) as unknown as MidnightOperationStep<K>,
      );
    },
    resumeOperation<K extends MidnightCommandKind>(
      operation: {
        readonly id: number;
        readonly generation: number;
        readonly commandKind: K;
      },
      result: MidnightNetworkResult | readonly MidnightNetworkResult[] | null,
    ): Promise<MidnightOperationStep<K>> {
      onResume(result);
      return Promise.resolve({
        kind: "complete",
        operation,
        result: decodeCommandResult(operation.commandKind, { values: [] }),
      });
    },
    cancelOperation() {
      onCancel?.();
      return Promise.resolve();
    },
  };
}

const unusedFetch: MidnightFetch = () => Promise.reject(new Error("unused"));

await test("a 3-effect batch resolves in one resumeOperation call carrying ordered results", async () => {
  const resumeCalls: (
    MidnightNetworkResult | readonly MidnightNetworkResult[] | null
  )[] = [];
  const executed: string[] = [];
  const api = completingApi(
    (commandKind) =>
      batchNetworkStep(commandKind, [
        { id: "e1", effect: "prove", body: Uint8Array.of(1) },
        { id: "e2", effect: "check", body: Uint8Array.of(2) },
        { id: "e3", effect: "prove", body: Uint8Array.of(3) },
      ]),
    (result) => resumeCalls.push(result),
  );
  const proofAdapter: InternalMidnightProofAdapter = {
    execute(effect, request) {
      executed.push(`${effect}:${String(request[0])}`);
      return Promise.resolve(Uint8Array.of((request[0] ?? 0) + 10));
    },
  };
  const transport = createMidnightTransportWithProofAdapter(
    transportConfig(unusedFetch),
    proofAdapter,
  );
  await transport.runCommand(
    api,
    { id: 1, generation: 1 },
    { kind: "parseCheckResult", resultBase64: "" },
  );
  assert.equal(resumeCalls.length, 1);
  const batch = resumeCalls[0];
  assert.equal(Array.isArray(batch), true);
  const results = batch as MidnightNetworkResult[];
  assert.deepEqual(
    results.map((result) => result.effectId),
    ["e1", "e2", "e3"],
  );
  assert.deepEqual(executed.sort(), ["check:2", "prove:1", "prove:3"]);
});

await test("a single-effect round still sends a bare object, not an array", async () => {
  const resumeCalls: (
    MidnightNetworkResult | readonly MidnightNetworkResult[] | null
  )[] = [];
  const api = completingApi(
    (commandKind) => ({
      kind: "network",
      operation: { id: 9, generation: 1, commandKind },
      effectId: "solo",
      effect: "prove",
      endpointRole: "proof",
      bodyBase64: encodeBase64(Uint8Array.of(7)),
    }),
    (result) => resumeCalls.push(result),
  );
  const proofAdapter: InternalMidnightProofAdapter = {
    execute: (_effect, request) => Promise.resolve(request),
  };
  const transport = createMidnightTransportWithProofAdapter(
    transportConfig(unusedFetch),
    proofAdapter,
  );
  await transport.runCommand(
    api,
    { id: 1, generation: 1 },
    { kind: "parseCheckResult", resultBase64: "" },
  );
  assert.equal(resumeCalls.length, 1);
  assert.equal(Array.isArray(resumeCalls[0]), false);
  assert.equal((resumeCalls[0] as MidnightNetworkResult).effectId, "solo");
});

await test("the concurrency limiter bounds in-flight proof calls", async () => {
  let active = 0;
  let peak = 0;
  const api = completingApi(
    (commandKind) =>
      batchNetworkStep(
        commandKind,
        Array.from({ length: 5 }, (_value, index) => ({
          id: `e${String(index)}`,
          effect: "prove" as const,
          body: Uint8Array.of(index),
        })),
      ),
    () => undefined,
  );
  const proofAdapter: InternalMidnightProofAdapter = {
    execute(_effect, request) {
      active += 1;
      peak = Math.max(peak, active);
      return new Promise((resolve) => {
        setTimeout(() => {
          active -= 1;
          resolve(request);
        }, 5);
      });
    },
  };
  const transport = createMidnightTransportWithProofAdapter(
    transportConfig(unusedFetch),
    proofAdapter,
  );
  await transport.runCommand(
    api,
    { id: 1, generation: 1 },
    { kind: "parseCheckResult", resultBase64: "" },
  );
  assert.equal(peak, 2);
  assert.equal(active, 0);
});

await test("a batch containing a non-proof effect runs sequentially", async () => {
  let active = 0;
  let peak = 0;
  const api = completingApi(
    (commandKind) =>
      rawMixedBatchStep(commandKind, [
        {
          effectId: "e1",
          effect: "prove",
          endpointRole: "proof",
          bodyBase64: encodeBase64(Uint8Array.of(1)),
        },
        {
          effectId: "e2",
          effect: "check",
          endpointRole: "indexer",
          bodyBase64: encodeBase64(Uint8Array.of(2)),
        },
        {
          effectId: "e3",
          effect: "prove",
          endpointRole: "proof",
          bodyBase64: encodeBase64(Uint8Array.of(3)),
        },
      ]),
    () => undefined,
  );
  const fetch: MidnightFetch = () => {
    active += 1;
    peak = Math.max(peak, active);
    return new Promise((resolve) => {
      setTimeout(() => {
        active -= 1;
        resolve({
          status: 200,
          arrayBuffer: () => Promise.resolve(new Uint8Array([9]).buffer),
        });
      }, 5);
    });
  };
  const proofAdapter: InternalMidnightProofAdapter = {
    execute(_effect, request) {
      active += 1;
      peak = Math.max(peak, active);
      return new Promise((resolve) => {
        setTimeout(() => {
          active -= 1;
          resolve(request);
        }, 5);
      });
    },
  };
  const transport = createMidnightTransportWithProofAdapter(
    transportConfig(fetch),
    proofAdapter,
  );
  await transport.runCommand(
    api,
    { id: 1, generation: 1 },
    { kind: "parseCheckResult", resultBase64: "" },
  );
  assert.equal(peak, 1);
  assert.equal(active, 0);
});

await test("abort mid-batch stops admission, awaits started calls, and throws CANCELLED once", async () => {
  const started: Uint8Array[] = [];
  const gates: (() => void)[] = [];
  let cancelCount = 0;
  let resumeCount = 0;
  const controller = new AbortController();
  const api: MidnightRuntimeApi = {
    ...idleApi(),
    beginCommand<K extends MidnightCommandKind>(
      _session: { readonly id: number; readonly generation: number },
      command: MidnightCommand<K>,
    ): Promise<MidnightOperationStep<K>> {
      return Promise.resolve(
        batchNetworkStep(command.kind, [
          { id: "e0", effect: "prove", body: Uint8Array.of(0) },
          { id: "e1", effect: "prove", body: Uint8Array.of(1) },
          { id: "e2", effect: "prove", body: Uint8Array.of(2) },
          { id: "e3", effect: "prove", body: Uint8Array.of(3) },
        ]),
      );
    },
    resumeOperation() {
      resumeCount += 1;
      return Promise.reject(new Error("resumeOperation must not be reached"));
    },
    cancelOperation() {
      cancelCount += 1;
      return Promise.resolve();
    },
  };
  const proofAdapter: InternalMidnightProofAdapter = {
    execute(_effect, request) {
      started.push(request);
      return new Promise((resolve) => {
        gates.push(() => {
          resolve(request);
        });
        if (started.length === 2) {
          controller.abort();
        }
      });
    },
  };
  const transport = createMidnightTransportWithProofAdapter(
    transportConfig(unusedFetch),
    proofAdapter,
  );
  const running = transport.runCommand(
    api,
    { id: 1, generation: 1 },
    { kind: "parseCheckResult", resultBase64: "" },
    { signal: controller.signal },
  );
  await new Promise((resolve) => setTimeout(resolve, 0));
  for (const release of gates) release();

  await assert.rejects(running, { code: "CANCELLED" });
  assert.equal(started.length, 2);
  assert.equal(cancelCount, 1);
  assert.equal(resumeCount, 0);
  for (const request of started) {
    assert.equal(
      Array.from(request).every((byte) => byte === 0),
      true,
    );
  }
});

await test("batch request bodies are zeroed after use on the success path", async () => {
  const captured: Uint8Array[] = [];
  const api = completingApi(
    (commandKind) =>
      batchNetworkStep(commandKind, [
        { id: "e0", effect: "prove", body: Uint8Array.of(11) },
        { id: "e1", effect: "check", body: Uint8Array.of(22) },
      ]),
    () => undefined,
  );
  const proofAdapter: InternalMidnightProofAdapter = {
    execute(_effect, request) {
      captured.push(request);
      return Promise.resolve(Uint8Array.of(1));
    },
  };
  const transport = createMidnightTransportWithProofAdapter(
    transportConfig(unusedFetch),
    proofAdapter,
  );
  await transport.runCommand(
    api,
    { id: 1, generation: 1 },
    { kind: "parseCheckResult", resultBase64: "" },
  );
  assert.equal(captured.length, 2);
  for (const request of captured) {
    assert.equal(
      Array.from(request).every((byte) => byte === 0),
      true,
    );
  }
});

await test("transport rejects a maximumConcurrentProofs configuration above the cap or non-positive", () => {
  const base = transportConfig(unusedFetch);
  for (const invalid of [0, -1, 1.5, 9, Number.POSITIVE_INFINITY]) {
    assert.throws(
      () =>
        createStandardMidnightTransport({
          ...base,
          maximumConcurrentProofs: invalid,
        }),
      { code: "INVALID_ARGUMENT" },
    );
  }
  assert.doesNotThrow(() =>
    createStandardMidnightTransport({ ...base, maximumConcurrentProofs: 8 }),
  );
});
