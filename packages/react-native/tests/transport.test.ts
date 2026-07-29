import assert from "node:assert/strict";
import test from "node:test";

import { encodeBase64 } from "../src/base64.js";
import type { MidnightCommand, MidnightCommandKind } from "../src/commands.js";
import { decodeCommandResult } from "../src/decode.js";
import { MidnightRuntimeError } from "../src/errors.js";
import type { MidnightLogger } from "../src/host.js";
import type {
  MidnightNetworkResult,
  MidnightOperationStep,
  MidnightRuntimeApi,
} from "../src/runtime-types.js";
import {
  createStandardMidnightTransport,
  type MidnightFetch,
  type MidnightWebSocket,
} from "../src/transport.js";

function socket(): MidnightWebSocket {
  return {
    readyState: 0,
    send(data) {
      void data;
    },
    close(code, reason) {
      void code;
      void reason;
    },
    addEventListener(type, listener) {
      void type;
      void listener;
    },
  };
}

function networkApi(onCancel: () => void): MidnightRuntimeApi {
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
    beginCommand<K extends MidnightCommandKind>(
      _session: { readonly id: number; readonly generation: number },
      command: MidnightCommand<K>,
    ): Promise<MidnightOperationStep<K>> {
      return Promise.resolve({
        kind: "network",
        operation: {
          id: 9,
          generation: 1,
          commandKind: command.kind,
        },
        effectId: "effect-1",
        effect: "submit",
        endpointRole: "node",
        bodyBase64: encodeBase64(new TextEncoder().encode("sensitive-payload")),
      });
    },
    resumeOperation() {
      return Promise.reject(new Error("unused"));
    },
    cancelOperation() {
      onCancel();
      return Promise.resolve();
    },
    closeWalletSession() {
      return Promise.resolve();
    },
  };
}

function config(fetch: MidnightFetch, logger: MidnightLogger) {
  return {
    network: {
      indexerHttpUrl: "https://indexer.example.invalid/request",
      indexerWebSocketUrl: "wss://indexer.example.invalid/stream",
      proofServerUrl: "https://proof.example.invalid/request",
      nodeUrl: "https://node.example.invalid/request",
    },
    fetch,
    createWebSocket: socket,
    logger,
  } as const;
}

await test("transport maps an ambiguous submission and redacts logs", async () => {
  const events: object[] = [];
  const logger: MidnightLogger = {
    write(event) {
      events.push(event);
    },
  };
  const fetch: MidnightFetch = () => {
    return Promise.reject(new Error("connection lost"));
  };
  let cancelled = 0;
  const transport = createStandardMidnightTransport(config(fetch, logger));
  const api = networkApi(() => {
    cancelled += 1;
  });

  await assert.rejects(
    transport.runCommand(
      api,
      { id: 1, generation: 1 },
      { kind: "submitFinalized", rawBase64: "AA==" },
    ),
  );
  assert.equal(cancelled, 1);
  const serialized = JSON.stringify(events);
  assert.equal(serialized.includes("sensitive-payload"), false);
  assert.equal(
    serialized.includes(
      encodeBase64(new TextEncoder().encode("sensitive-payload")),
    ),
    false,
  );
  assert.match(serialized, /SUBMISSION_STATUS_UNKNOWN/);
});

await test("caller cancellation aborts transport and cancels native operation", async () => {
  const logger: MidnightLogger = {
    write(event) {
      void event;
    },
  };
  const fetch: MidnightFetch = (_url, request) => {
    if (request.signal.aborted) {
      return Promise.reject(new MidnightRuntimeError("CANCELLED"));
    }
    return new Promise((_resolve, reject) => {
      request.signal.addEventListener(
        "abort",
        () => {
          reject(new MidnightRuntimeError("CANCELLED"));
        },
        { once: true },
      );
    });
  };
  let cancelled = 0;
  const transport = createStandardMidnightTransport(config(fetch, logger));
  const api = networkApi(() => {
    cancelled += 1;
  });
  const controller = new AbortController();
  const running = transport.runCommand(
    api,
    { id: 1, generation: 1 },
    { kind: "submitFinalized", rawBase64: "AA==" },
    { signal: controller.signal },
  );
  controller.abort();

  await assert.rejects(running, { message: "CANCELLED" });
  assert.equal(cancelled, 1);
});

await test("standard transport forwards one correlated network result", async () => {
  let received: MidnightNetworkResult | null = null;
  const api: MidnightRuntimeApi = {
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
    beginCommand<K extends MidnightCommandKind>(
      _session: { readonly id: number; readonly generation: number },
      command: MidnightCommand<K>,
    ): Promise<MidnightOperationStep<K>> {
      return Promise.resolve({
        kind: "network",
        operation: { id: 9, generation: 1, commandKind: command.kind },
        effectId: "1:9:1",
        effect: "submit",
        endpointRole: "node",
        bodyBase64: "AA==",
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
      received = result;
      return Promise.resolve({
        kind: "complete",
        operation,
        result: decodeCommandResult(operation.commandKind, {
          transactionHash: "aa".repeat(32),
          identifiers: [],
          status: "accepted",
        }),
      });
    },
    cancelOperation() {
      return Promise.resolve();
    },
    closeWalletSession() {
      return Promise.resolve();
    },
  };
  const fetch: MidnightFetch = () =>
    Promise.resolve({
      status: 200,
      arrayBuffer() {
        return Promise.resolve(new Uint8Array([7, 8]).buffer);
      },
    });
  const logger: MidnightLogger = {
    write(event) {
      void event;
    },
  };
  const transport = createStandardMidnightTransport(config(fetch, logger));

  const result = await transport.runCommand(
    api,
    { id: 1, generation: 1 },
    { kind: "submitFinalized", rawBase64: "AA==" },
  );

  assert.deepEqual(received, {
    effectId: "1:9:1",
    outcome: "accepted",
    bodyBase64: "Bwg=",
  });
  assert.equal(result.status, "accepted");
});
