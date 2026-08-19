import assert from "node:assert/strict";
import test from "node:test";

import {
  createMidnightLocalProver,
  MAX_PROOF_CONCURRENCY,
  MIN_PROOF_CONCURRENCY,
  type MidnightLocalProverConfiguration,
  type NativeLocalProverModule,
} from "../src/local-prover.js";

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

function nativeFixture(): NativeLocalProverModule {
  return {
    configure: () => Promise.resolve(1),
    check: () => Promise.resolve(new Uint8Array([1])),
    prove: () => Promise.resolve(new Uint8Array([2])),
    close: () => Promise.resolve(),
  };
}

await test("local prover exposes the admission limit and rejects unusable ones", async () => {
  const module = nativeFixture();
  const limits: number[] = [];
  module.setMaxConcurrency = (limit) => {
    limits.push(limit);
    return Promise.resolve();
  };
  const prover = await createMidnightLocalProver(configuration, {
    nativeModuleLoader: () => module,
  });
  assert.equal(typeof prover.setMaxConcurrency, "function");

  // Pinning to 1 is what an attribution run needs; the default ceiling is 4.
  await prover.setMaxConcurrency?.(MIN_PROOF_CONCURRENCY);
  await prover.setMaxConcurrency?.(MAX_PROOF_CONCURRENCY);
  assert.deepEqual(limits, [MIN_PROOF_CONCURRENCY, MAX_PROOF_CONCURRENCY]);

  // Native clamps silently, which would let a run believe it pinned a limit it
  // never got. Rejecting keeps the requested limit and the applied one identical.
  for (const unusable of [0, MAX_PROOF_CONCURRENCY + 1, 1.5, Number.NaN]) {
    await assert.rejects(
      () => prover.setMaxConcurrency?.(unusable) ?? Promise.resolve(),
      { code: "INVALID_REQUEST" },
      `limit ${String(unusable)}`,
    );
  }
  assert.deepEqual(limits, [MIN_PROOF_CONCURRENCY, MAX_PROOF_CONCURRENCY]);

  // Instrumentation, not proving: still callable once the registry is closed.
  await prover.close();
  await prover.setMaxConcurrency?.(MIN_PROOF_CONCURRENCY);
  assert.equal(limits.length, 3);
});

await test("local prover omits the admission limit on older native binaries", async () => {
  const module = nativeFixture();
  const prover = await createMidnightLocalProver(configuration, {
    nativeModuleLoader: () => module,
  });
  assert.equal(typeof prover.setMaxConcurrency, "undefined");
});
