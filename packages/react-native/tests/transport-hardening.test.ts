import assert from "node:assert/strict";
import test from "node:test";

import type { MidnightCommand, MidnightCommandKind } from "../src/commands.js";
import { decodeCommandResult } from "../src/decode.js";
import { silentMidnightLogger } from "../src/host.js";
import type {
  MidnightEndpointRole,
  MidnightNetworkEffect,
  MidnightNetworkResult,
  MidnightOperationStep,
  MidnightRuntimeApi,
} from "../src/runtime-types.js";
import {
  createStandardMidnightTransport,
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

function resumableNetworkApi(
  effect: MidnightNetworkEffect,
  role: MidnightEndpointRole,
  onResume: (result: MidnightNetworkResult | null) => void,
): MidnightRuntimeApi {
  return {
    ...idleApi(),
    beginCommand<K extends MidnightCommandKind>(
      _session: { readonly id: number; readonly generation: number },
      command: MidnightCommand<K>,
    ): Promise<MidnightOperationStep<K>> {
      return Promise.resolve({
        kind: "network",
        operation: { id: 7, generation: 1, commandKind: command.kind },
        effectId: `${role}-${effect}`,
        effect,
        endpointRole: role,
        bodyBase64: "",
      });
    },
    resumeOperation<K extends MidnightCommandKind>(
      operation: {
        readonly id: number;
        readonly generation: number;
        readonly commandKind: K;
      },
      result: MidnightNetworkResult | null,
    ): Promise<MidnightOperationStep<K>> {
      onResume(result);
      return Promise.resolve({
        kind: "complete",
        operation,
        result: decodeCommandResult(operation.commandKind, { values: ["1"] }),
      });
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

await test("transport rejects malformed and unsupported endpoint URLs", () => {
  const fetch: MidnightFetch = () => Promise.reject(new Error("unused"));
  const base = transportConfig(fetch);
  assert.throws(
    () =>
      createStandardMidnightTransport({
        ...base,
        network: { ...base.network, indexerHttpUrl: "not a url" },
      }),
    { code: "INVALID_ARGUMENT" },
  );
  assert.throws(
    () =>
      createStandardMidnightTransport({
        ...base,
        network: { ...base.network, proofServerUrl: "ftp://example.invalid" },
      }),
    { code: "INVALID_ARGUMENT" },
  );
});

await test("transport forwards rejected indexer and proof responses with headers", async () => {
  const requests: string[] = [];
  const fetch: MidnightFetch = (url, request) => {
    requests.push(`${url}:${request.headers.authorization ?? ""}`);
    return Promise.resolve({
      status: 422,
      arrayBuffer: () => Promise.resolve(new ArrayBuffer(0)),
    });
  };
  const base = transportConfig(fetch);
  const transport = createStandardMidnightTransport({
    ...base,
    headers: (role) => Promise.resolve({ authorization: `Bearer ${role}` }),
  });
  for (const role of ["indexer", "proof"] as const) {
    const api = resumableNetworkApi("check", role, (result) => {
      assert.equal(result?.outcome, "rejected");
      assert.equal(result.bodyBase64, undefined);
    });
    const result = await transport.runCommand(
      api,
      { id: 1, generation: 1 },
      { kind: "parseCheckResult", resultBase64: "" },
    );
    assert.deepEqual(result.values, ["1"]);
  }
  assert.equal(requests[0]?.includes("indexer.example.invalid"), true);
  assert.equal(requests[1]?.includes("proof.example.invalid"), true);
});

await test("transport separates ambiguous submission from other failures", async () => {
  const unavailable: MidnightFetch = () =>
    Promise.resolve({
      status: 503,
      arrayBuffer: () => Promise.resolve(new ArrayBuffer(0)),
    });
  const transport = createStandardMidnightTransport(
    transportConfig(unavailable),
  );
  let submissionOutcome = "";
  const submitApi = resumableNetworkApi("submit", "node", (result) => {
    submissionOutcome = result?.outcome ?? "";
  });
  await transport.runCommand(
    submitApi,
    { id: 1, generation: 1 },
    { kind: "parseCheckResult", resultBase64: "" },
  );
  assert.equal(submissionOutcome, "statusUnknown");

  let cancelled = 0;
  const checkApi = {
    ...resumableNetworkApi("check", "node", () => undefined),
    cancelOperation() {
      cancelled += 1;
      return Promise.resolve();
    },
  };
  await assert.rejects(
    transport.runCommand(
      checkApi,
      { id: 1, generation: 1 },
      { kind: "parseCheckResult", resultBase64: "" },
    ),
    { code: "TRANSPORT_ERROR" },
  );
  assert.equal(cancelled, 1);
});

await test("transport normalizes generic non-submission network failure", async () => {
  const failed: MidnightFetch = () =>
    Promise.reject(new Error("connection unavailable"));
  const transport = createStandardMidnightTransport(transportConfig(failed));
  await assert.rejects(
    transport.runCommand(
      resumableNetworkApi("prove", "proof", () => undefined),
      { id: 1, generation: 1 },
      { kind: "parseCheckResult", resultBase64: "" },
    ),
    { code: "TRANSPORT_ERROR" },
  );
});

await test("transport handles progress and completion", async () => {
  let resumedWith: MidnightNetworkResult | null | undefined;
  const api: MidnightRuntimeApi = {
    ...idleApi(),
    beginCommand<K extends MidnightCommandKind>(
      _session: { readonly id: number; readonly generation: number },
      command: MidnightCommand<K>,
    ): Promise<MidnightOperationStep<K>> {
      return Promise.resolve({
        kind: "progress",
        operation: { id: 2, generation: 1, commandKind: command.kind },
        phase: "queued",
      });
    },
    resumeOperation<K extends MidnightCommandKind>(
      operation: {
        readonly id: number;
        readonly generation: number;
        readonly commandKind: K;
      },
      result: MidnightNetworkResult | null,
    ): Promise<MidnightOperationStep<K>> {
      resumedWith = result;
      return Promise.resolve({
        kind: "complete",
        operation,
        result: decodeCommandResult(operation.commandKind, { values: [] }),
      });
    },
  };
  const unusedFetch: MidnightFetch = () => Promise.reject(new Error("unused"));
  const transport = createStandardMidnightTransport(
    transportConfig(unusedFetch),
  );
  const result = await transport.runCommand(
    api,
    { id: 1, generation: 1 },
    { kind: "parseCheckResult", resultBase64: "" },
  );
  assert.deepEqual(result.values, []);
  assert.equal(resumedWith, null);
});

await test("transport cancels when the maximum effect step count is reached", async () => {
  let cancelled = 0;
  const api: MidnightRuntimeApi = {
    ...idleApi(),
    beginCommand<K extends MidnightCommandKind>(
      _session: { readonly id: number; readonly generation: number },
      command: MidnightCommand<K>,
    ): Promise<MidnightOperationStep<K>> {
      return Promise.resolve({
        kind: "progress",
        operation: { id: 3, generation: 1, commandKind: command.kind },
        phase: "one",
      });
    },
    resumeOperation<K extends MidnightCommandKind>(operation: {
      readonly id: number;
      readonly generation: number;
      readonly commandKind: K;
    }): Promise<MidnightOperationStep<K>> {
      return Promise.resolve({ kind: "progress", operation, phase: "two" });
    },
    cancelOperation() {
      cancelled += 1;
      return Promise.reject(new Error("already cancelled"));
    },
  };
  const unusedFetch: MidnightFetch = () => Promise.reject(new Error("unused"));
  const transport = createStandardMidnightTransport({
    ...transportConfig(unusedFetch),
    maximumEffectSteps: 1,
  });
  await assert.rejects(
    transport.runCommand(
      api,
      { id: 1, generation: 1 },
      { kind: "parseCheckResult", resultBase64: "" },
    ),
    { code: "NATIVE_INTERNAL" },
  );
  assert.equal(cancelled, 1);
});

await test("transport returns an immediately complete operation", async () => {
  const api: MidnightRuntimeApi = {
    ...idleApi(),
    beginCommand<K extends MidnightCommandKind>(
      _session: { readonly id: number; readonly generation: number },
      command: MidnightCommand<K>,
    ): Promise<MidnightOperationStep<K>> {
      return Promise.resolve({
        kind: "complete",
        operation: null,
        result: decodeCommandResult(command.kind, { values: ["2"] }),
      });
    },
  };
  const unusedFetch: MidnightFetch = () => Promise.reject(new Error("unused"));
  const transport = createStandardMidnightTransport(
    transportConfig(unusedFetch),
  );
  const result = await transport.runCommand(
    api,
    { id: 1, generation: 1 },
    { kind: "parseCheckResult", resultBase64: "" },
  );
  assert.deepEqual(result.values, ["2"]);
});

await test("sync socket decodes both payload forms and closes on abort", () => {
  const recording = new RecordingSocket();
  const unusedFetch: MidnightFetch = () => Promise.reject(new Error("unused"));
  const base = transportConfig(unusedFetch);
  const transport = createStandardMidnightTransport({
    ...base,
    createWebSocket(url) {
      assert.equal(url, base.network.indexerWebSocketUrl);
      return recording;
    },
  });
  const payloads: Uint8Array[] = [];
  const aborter = new AbortController();
  const opened = transport.openSyncSocket((payload) => {
    payloads.push(payload);
  }, aborter.signal);
  opened.send("ping");
  recording.emit("AQ==");
  recording.emit(new Uint8Array([2]).buffer);
  aborter.abort();
  assert.equal(recording.sent, "ping");
  assert.deepEqual(payloads, [new Uint8Array([1]), new Uint8Array([2])]);
  assert.deepEqual(recording.closed, [1000, "cancelled"]);
});
