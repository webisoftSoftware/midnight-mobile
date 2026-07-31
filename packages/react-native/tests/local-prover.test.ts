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
import type {
  MidnightFetch,
  MidnightTransportConfiguration,
  MidnightWebSocket,
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

function socket(): MidnightWebSocket {
  return {
    readyState: 0,
    send() {
      return undefined;
    },
    close() {
      return undefined;
    },
    addEventListener() {
      return undefined;
    },
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
