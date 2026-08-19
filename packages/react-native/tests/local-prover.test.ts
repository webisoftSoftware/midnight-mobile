import assert from "node:assert/strict";
import test from "node:test";

import { encodeBase64 } from "../src/base64.js";
import type { MidnightCommand, MidnightCommandKind } from "../src/commands.js";
import {
  createLocalProverMidnightTransport,
  createMidnightLocalProver,
  MidnightLocalProverError,
  type MidnightLocalProver,
  type MidnightLocalProverConfiguration,
  type NativeLocalProverModule,
} from "../src/local-prover.js";
import type {
  MidnightNetworkEffect,
  MidnightNetworkResult,
  MidnightOperationHandle,
  MidnightOperationStep,
  MidnightRuntimeApi,
  MidnightSessionHandle,
} from "../src/runtime-types.js";
import {
  createStandardMidnightTransport,
  type MidnightFetch,
  type MidnightTransportConfiguration,
  type MidnightWebSocket,
} from "../src/transport.js";

const file = {
  uri: "asset://zswap/9/spend.bzkir",
  size: 1_294,
  sha256: "7cb5bbcf67cb212a3336fb439a77e8f32f0aa8a56185c8e1247d6cbfc7300205",
} as const;

const configuration: MidnightLocalProverConfiguration = {
  parameters: [{ k: 15, file: { ...file, uri: "asset://bls_midnight_2p15" } }],
  circuits: [
    {
      keyLocation: "midnight/zswap/spend",
      proverKey: { ...file, uri: "asset://zswap/9/spend.prover" },
      verifierKey: { ...file, uri: "asset://zswap/9/spend.verifier" },
      ir: file,
    },
  ],
};

function nativeFixture(): {
  readonly calls: string[];
  readonly requests: Uint8Array[];
  readonly module: NativeLocalProverModule;
} {
  const calls: string[] = [];
  const requests: Uint8Array[] = [];
  return {
    calls,
    requests,
    module: {
      configure(json) {
        calls.push(`configure:${json}`);
        return Promise.resolve(1);
      },
      check(request) {
        requests.push(request);
        calls.push(`check:${String(request.length)}`);
        return Promise.resolve(new Uint8Array([1]));
      },
      prove(request) {
        requests.push(request);
        calls.push(`prove:${String(request.length)}`);
        return Promise.resolve(new Uint8Array([2]));
      },
      close() {
        calls.push("close");
        return Promise.resolve();
      },
    },
  };
}

await test("local prover validates, configures, executes, and closes", async () => {
  const fixture = nativeFixture();
  const prover = await createMidnightLocalProver(configuration, {
    nativeModuleLoader: () => fixture.module,
  });
  const checkRequest = new Uint8Array([1, 2]);
  const proveRequest = new Uint8Array([3]);
  assert.deepEqual(await prover.check(checkRequest), new Uint8Array([1]));
  assert.deepEqual(await prover.prove(proveRequest), new Uint8Array([2]));
  assert.deepEqual(checkRequest, new Uint8Array([1, 2]));
  assert.deepEqual(proveRequest, new Uint8Array([3]));
  assert.deepEqual(fixture.requests, [new Uint8Array(2), new Uint8Array(1)]);
  await prover.close();
  await prover.close();
  assert.equal(fixture.calls.filter((call) => call === "close").length, 1);
  assert.match(fixture.calls[0] ?? "", /^configure:/u);
  await assert.rejects(prover.prove(new Uint8Array([1])), {
    code: "STALE_REGISTRY",
  });
});

await test("local prover rejects malformed configuration and maps native errors", async () => {
  const fixture = nativeFixture();
  await assert.rejects(
    createMidnightLocalProver(
      { ...configuration, parameters: [] },
      { nativeModuleLoader: () => fixture.module },
    ),
    { code: "INVALID_CONFIGURATION" },
  );
  let rejectedCopy: Uint8Array | undefined;
  fixture.module.prove = (request) => {
    rejectedCopy = request;
    return Promise.reject(
      Object.assign(new Error("native proof failed"), { code: "PROOF_FAILED" }),
    );
  };
  const prover = await createMidnightLocalProver(configuration, {
    nativeModuleLoader: () => fixture.module,
  });
  await assert.rejects(prover.prove(new Uint8Array([1])), {
    code: "PROOF_FAILED",
  });
  assert.deepEqual(rejectedCopy, new Uint8Array(1));
  assert.equal(
    new MidnightLocalProverError("CHECK_FAILED").message,
    "CHECK_FAILED",
  );
});

await test("local prover surfaces the circuit size behind a CIRCUIT_TOO_LARGE refusal", async () => {
  const fixture = nativeFixture();
  // The bridges append the size to the code string because the C ABI has one i32
  // and Expo's Exception has no other field that reaches JS.
  fixture.module.prove = () =>
    Promise.reject(
      Object.assign(new Error("refused"), { code: "CIRCUIT_TOO_LARGE k=18" }),
    );
  const prover = await createMidnightLocalProver(configuration, {
    nativeModuleLoader: () => fixture.module,
  });

  await assert.rejects(prover.prove(new Uint8Array([1])), {
    code: "CIRCUIT_TOO_LARGE",
    requiredK: 18,
  });
});

await test("local prover does not invent a circuit size it was not given", async () => {
  const fixture = nativeFixture();
  // A bare code from an older native binary must still map, just without a size —
  // the wallet's prompt falls back to generic wording rather than showing "k=NaN".
  fixture.module.prove = () =>
    Promise.reject(
      Object.assign(new Error("refused"), { code: "CIRCUIT_TOO_LARGE" }),
    );
  const prover = await createMidnightLocalProver(configuration, {
    nativeModuleLoader: () => fixture.module,
  });

  const error = await prover
    .prove(new Uint8Array([1]))
    .then(() => undefined)
    .catch((thrown: unknown) => thrown);
  assert.equal((error as MidnightLocalProverError).code, "CIRCUIT_TOO_LARGE");
  assert.equal((error as MidnightLocalProverError).requiredK, undefined);
});

function socket(): MidnightWebSocket {
  return {
    readyState: 0,
    send: () => undefined,
    close: () => undefined,
    addEventListener: () => undefined,
  };
}

function transportConfig(fetch: MidnightFetch): MidnightTransportConfiguration {
  return {
    network: {
      indexerHttpUrl: "https://indexer.invalid/request",
      indexerWebSocketUrl: "wss://indexer.invalid/stream",
      proofServerUrl: "https://proof.invalid/request",
      nodeUrl: "https://node.invalid/request",
    },
    fetch,
    createWebSocket: socket,
  };
}

function effectApi(effect: MidnightNetworkEffect): MidnightRuntimeApi {
  return {
    openWalletSession: () => Promise.reject(new Error("unused")),
    applySyncBatch: () => Promise.reject(new Error("unused")),
    getWalletSnapshot: () => Promise.reject(new Error("unused")),
    exportWalletCheckpoint: () => Promise.reject(new Error("unused")),
    beginCommand<K extends MidnightCommandKind>(
      _session: MidnightSessionHandle,
      command: MidnightCommand<K>,
    ): Promise<MidnightOperationStep<K>> {
      return Promise.resolve({
        kind: "network",
        operation: { id: 1, generation: 1, commandKind: command.kind },
        effectId: "effect",
        effect,
        endpointRole: "proof",
        bodyBase64: encodeBase64(new Uint8Array([7])),
      });
    },
    resumeOperation<K extends MidnightCommandKind>(
      _operation: MidnightOperationHandle<K>,
      networkResult: MidnightNetworkResult | null,
    ): Promise<MidnightOperationStep<K>> {
      assert.equal(networkResult?.outcome, "accepted");
      return Promise.resolve({
        kind: "complete",
        operation: null,
        result: { signatureHex: "00" },
      } as unknown as MidnightOperationStep<K>);
    },
    cancelOperation: () => Promise.resolve(),
    closeWalletSession: () => Promise.resolve(),
  };
}

await test("transport keeps only check and prove local", async () => {
  const localCalls: string[] = [];
  const prover: MidnightLocalProver = {
    configure: () => Promise.resolve(),
    check: (request) => {
      localCalls.push(`check:${String(request[0])}`);
      return Promise.resolve(new Uint8Array([8]));
    },
    prove: (request) => {
      localCalls.push(`prove:${String(request[0])}`);
      return Promise.resolve(new Uint8Array([9]));
    },
    close: () => Promise.resolve(),
  };
  let remoteCalls = 0;
  const fetch: MidnightFetch = () => {
    remoteCalls += 1;
    return Promise.resolve({
      status: 200,
      arrayBuffer: () => Promise.resolve(new Uint8Array([10]).buffer),
    });
  };
  const transport = createLocalProverMidnightTransport(
    transportConfig(fetch),
    prover,
  );
  const command = {
    kind: "signData",
    domain: "test",
    dataBase64: "AA==",
  } as const;
  await transport.runCommand(
    effectApi("check"),
    { id: 1, generation: 1 },
    command,
  );
  await transport.runCommand(
    effectApi("prove"),
    { id: 1, generation: 1 },
    command,
  );
  await transport.runCommand(
    effectApi("proveAndBalance"),
    { id: 1, generation: 1 },
    command,
  );
  await transport.runCommand(
    effectApi("balance"),
    { id: 1, generation: 1 },
    command,
  );
  assert.deepEqual(localCalls, ["check:7", "prove:7"]);
  assert.equal(remoteCalls, 2);
});

function batchProofApi(
  effects: readonly { readonly id: string; readonly body: Uint8Array }[],
  onResume: (
    result: MidnightNetworkResult | readonly MidnightNetworkResult[] | null,
  ) => void,
): MidnightRuntimeApi {
  const first = effects[0];
  if (first === undefined) throw new Error("effects must be non-empty");
  return {
    openWalletSession: () => Promise.reject(new Error("unused")),
    applySyncBatch: () => Promise.reject(new Error("unused")),
    getWalletSnapshot: () => Promise.reject(new Error("unused")),
    exportWalletCheckpoint: () => Promise.reject(new Error("unused")),
    beginCommand<K extends MidnightCommandKind>(
      _session: MidnightSessionHandle,
      command: MidnightCommand<K>,
    ): Promise<MidnightOperationStep<K>> {
      return Promise.resolve({
        kind: "network",
        operation: { id: 1, generation: 1, commandKind: command.kind },
        effectId: first.id,
        effect: "prove",
        endpointRole: "proof",
        bodyBase64: encodeBase64(first.body),
        effects: effects.map((entry) => ({
          effectId: entry.id,
          effect: "prove" as const,
          endpointRole: "proof" as const,
          bodyBase64: encodeBase64(entry.body),
        })),
      });
    },
    resumeOperation<K extends MidnightCommandKind>(
      _operation: MidnightOperationHandle<K>,
      result: MidnightNetworkResult | readonly MidnightNetworkResult[] | null,
    ): Promise<MidnightOperationStep<K>> {
      onResume(result);
      return Promise.resolve({
        kind: "complete",
        operation: null,
        result: { signatureHex: "00" },
      } as unknown as MidnightOperationStep<K>);
    },
    cancelOperation: () => Promise.resolve(),
    closeWalletSession: () => Promise.resolve(),
  };
}

await test("local prover exposes proveBatch only when the native module implements it", async () => {
  const fixture = nativeFixture();
  const withoutBatch = await createMidnightLocalProver(configuration, {
    nativeModuleLoader: () => fixture.module,
  });
  assert.equal(typeof withoutBatch.proveBatch, "undefined");

  fixture.module.proveBatch = (requests) =>
    Promise.resolve(
      requests.map((request) => new Uint8Array([request[0] ?? 0])),
    );
  const withBatch = await createMidnightLocalProver(configuration, {
    nativeModuleLoader: () => fixture.module,
  });
  assert.equal(typeof withBatch.proveBatch, "function");
});

await test("proveBatch copies each request, wipes every copy, and maps native errors", async () => {
  const fixture = nativeFixture();
  const seenCopies: Uint8Array[] = [];
  fixture.module.proveBatch = (requests) => {
    for (const request of requests) seenCopies.push(request);
    return Promise.resolve(
      requests.map((request) => new Uint8Array([(request[0] ?? 0) + 1])),
    );
  };
  const prover = await createMidnightLocalProver(configuration, {
    nativeModuleLoader: () => fixture.module,
  });
  const requestA = new Uint8Array([5]);
  const requestB = new Uint8Array([6]);
  const results = await prover.proveBatch?.([requestA, requestB]);
  assert.deepEqual(results, [new Uint8Array([6]), new Uint8Array([7])]);
  // originals are untouched, native received copies, and those copies are
  // wiped afterwards
  assert.deepEqual(requestA, new Uint8Array([5]));
  assert.deepEqual(requestB, new Uint8Array([6]));
  assert.equal(seenCopies.length, 2);
  for (const copy of seenCopies) {
    assert.equal(
      Array.from(copy).every((byte) => byte === 0),
      true,
    );
  }

  fixture.module.proveBatch = () =>
    Promise.reject(
      Object.assign(new Error("native batch failed"), { code: "PROOF_FAILED" }),
    );
  await assert.rejects(
    prover.proveBatch?.([new Uint8Array([1])]) ?? Promise.resolve(),
    {
      code: "PROOF_FAILED",
    },
  );
});

await test("createLocalProverMidnightTransport uses the native batch entrypoint for a prove-only batch", async () => {
  const fixture = nativeFixture();
  const batchCalls: (readonly Uint8Array[])[] = [];
  fixture.module.proveBatch = (requests) => {
    batchCalls.push(requests.map((request) => request.slice()));
    return Promise.resolve(
      requests.map((request) => new Uint8Array([(request[0] ?? 0) + 100])),
    );
  };
  const prover = await createMidnightLocalProver(configuration, {
    nativeModuleLoader: () => fixture.module,
  });
  const resumeCalls: (
    MidnightNetworkResult | readonly MidnightNetworkResult[] | null
  )[] = [];
  const api = batchProofApi(
    [
      { id: "p0", body: Uint8Array.of(1) },
      { id: "p1", body: Uint8Array.of(2) },
    ],
    (result) => resumeCalls.push(result),
  );
  const transport = createLocalProverMidnightTransport(
    transportConfig(() => Promise.reject(new Error("unused"))),
    prover,
  );
  await transport.runCommand(api, { id: 1, generation: 1 }, {
    kind: "signData",
    domain: "test",
    dataBase64: "AA==",
  } as const);
  assert.equal(batchCalls.length, 1);
  assert.equal(
    fixture.calls.filter((call) => call.startsWith("prove:")).length,
    0,
  );
  const results = resumeCalls[0];
  assert.equal(Array.isArray(results), true);
  assert.deepEqual(
    (results as MidnightNetworkResult[]).map((result) => result.effectId),
    ["p0", "p1"],
  );
});

await test("createLocalProverMidnightTransport falls back to individual prove calls without a native batch entrypoint", async () => {
  const fixture = nativeFixture();
  const prover = await createMidnightLocalProver(configuration, {
    nativeModuleLoader: () => fixture.module,
  });
  assert.equal(typeof prover.proveBatch, "undefined");
  const resumeCalls: (
    MidnightNetworkResult | readonly MidnightNetworkResult[] | null
  )[] = [];
  const api = batchProofApi(
    [
      { id: "p0", body: Uint8Array.of(3) },
      { id: "p1", body: Uint8Array.of(4) },
    ],
    (result) => resumeCalls.push(result),
  );
  const transport = createLocalProverMidnightTransport(
    transportConfig(() => Promise.reject(new Error("unused"))),
    prover,
  );
  await transport.runCommand(api, { id: 1, generation: 1 }, {
    kind: "signData",
    domain: "test",
    dataBase64: "AA==",
  } as const);
  assert.equal(
    fixture.calls.filter((call) => call.startsWith("prove:")).length,
    2,
  );
  const results = resumeCalls[0];
  assert.equal(Array.isArray(results), true);
  assert.deepEqual(
    (results as MidnightNetworkResult[]).map((result) => result.effectId),
    ["p0", "p1"],
  );
});

await test("standard transport routes proof effects to exact service paths", async () => {
  const urls: string[] = [];
  const fetch: MidnightFetch = (url) => {
    urls.push(url);
    return Promise.resolve({
      status: 200,
      arrayBuffer: () => Promise.resolve(new ArrayBuffer(0)),
    });
  };
  const transport = createStandardMidnightTransport(transportConfig(fetch));
  const command = {
    kind: "signData",
    domain: "test",
    dataBase64: "AA==",
  } as const;
  for (const effect of [
    "check",
    "prove",
    "proveAndBalance",
    "balance",
  ] as const) {
    await transport.runCommand(
      effectApi(effect),
      { id: 1, generation: 1 },
      command,
    );
  }
  assert.deepEqual(urls, [
    "https://proof.invalid/request/check",
    "https://proof.invalid/request/prove",
    "https://proof.invalid/request/prove-and-balance",
    "https://proof.invalid/request/balance-only",
  ]);
});
