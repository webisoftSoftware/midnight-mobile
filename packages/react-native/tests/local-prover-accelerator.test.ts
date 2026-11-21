import assert from "node:assert/strict";
import test from "node:test";

import {
  createMidnightLocalProver,
  MidnightLocalProverError,
  type MidnightLocalProverConfiguration,
  type NativeLocalProverModule,
} from "../src/local-prover.js";
import { decodeAcceleratorStatus } from "../src/local-prover-accelerator.js";

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

// The object the native prover reports with a GPU in use.
const activeStatus = {
  requested: true,
  availability: "active",
  active: true,
  tripped: false,
  cpuRetries: 0,
  device: "Apple A18 Pro GPU",
  gpuOperations: 72,
  cpuOperations: 10,
  deviceMicros: 181_235,
  tables: 2,
  tablePoints: 65_536,
} as const;

await test("local prover turns the accelerator on and off through the native switch", async () => {
  const module = nativeFixture();
  const switched: boolean[] = [];
  module.setAccelerator = (enabled) => {
    switched.push(enabled);
    return Promise.resolve();
  };
  const prover = await createMidnightLocalProver(configuration, {
    nativeModuleLoader: () => module,
  });

  await prover.setAccelerator?.(true);
  await prover.setAccelerator?.(false);
  assert.deepEqual(switched, [true, false]);

  // A truthy non-boolean must not silently enable GPU proving.
  await assert.rejects(
    () =>
      prover.setAccelerator?.("yes" as unknown as boolean) ?? Promise.resolve(),
    (error) =>
      error instanceof MidnightLocalProverError &&
      error.code === "INVALID_REQUEST",
  );
  assert.deepEqual(switched, [true, false]);

  // The switch is the process's, so it stays usable on a closed prover.
  await prover.close();
  await prover.setAccelerator?.(true);
  assert.deepEqual(switched, [true, false, true]);
});

await test("local prover reports the accelerator status the native prover gives", async () => {
  const module = nativeFixture();
  module.acceleratorStatus = () =>
    Promise.resolve(JSON.stringify(activeStatus));
  const prover = await createMidnightLocalProver(configuration, {
    nativeModuleLoader: () => module,
  });

  assert.deepEqual(await prover.getAcceleratorStatus?.(), activeStatus);
});

await test("local prover refuses a status it does not fully understand", async () => {
  const module = nativeFixture();
  let reported = "";
  module.acceleratorStatus = () => Promise.resolve(reported);
  const prover = await createMidnightLocalProver(configuration, {
    nativeModuleLoader: () => module,
  });

  const malformed = [
    "",
    "[]",
    "null",
    "{}",
    JSON.stringify({ ...activeStatus, availability: "probably" }),
    JSON.stringify({ ...activeStatus, active: "true" }),
    JSON.stringify({ ...activeStatus, gpuOperations: -1 }),
    JSON.stringify({ ...activeStatus, gpuOperations: 1.5 }),
    JSON.stringify({ ...activeStatus, device: 7 }),
    JSON.stringify({ ...activeStatus, tablePoints: undefined }),
  ];
  for (const status of malformed) {
    reported = status;
    await assert.rejects(
      () => prover.getAcceleratorStatus?.() ?? Promise.resolve(),
      (error) =>
        error instanceof MidnightLocalProverError &&
        error.code === "NATIVE_INTERNAL",
      `status ${status}`,
    );
  }
});

await test("local prover maps a native accelerator failure to a typed error", async () => {
  const module = nativeFixture();
  module.setAccelerator = () =>
    Promise.reject(Object.assign(new Error("native"), { code: "PROVER_BUSY" }));
  module.acceleratorStatus = () => Promise.reject(new Error("unexpected"));
  const prover = await createMidnightLocalProver(configuration, {
    nativeModuleLoader: () => module,
  });

  await assert.rejects(
    () => prover.setAccelerator?.(true) ?? Promise.resolve(),
    (error) =>
      error instanceof MidnightLocalProverError && error.code === "PROVER_BUSY",
  );
  await assert.rejects(
    () => prover.getAcceleratorStatus?.() ?? Promise.resolve(),
    (error) =>
      error instanceof MidnightLocalProverError &&
      error.code === "NATIVE_INTERNAL",
  );
});

await test("local prover omits the accelerator on older native binaries", async () => {
  const prover = await createMidnightLocalProver(configuration, {
    nativeModuleLoader: nativeFixture,
  });

  assert.equal(typeof prover.setAccelerator, "undefined");
  assert.equal(typeof prover.getAcceleratorStatus, "undefined");
});

await test("accelerator status decoding accepts every reported availability", () => {
  for (const availability of [
    "not-requested",
    "active",
    "unsupported",
    "no-device",
    "hooks-taken",
  ] as const) {
    const status = { ...activeStatus, availability, device: null };
    assert.deepEqual(decodeAcceleratorStatus(JSON.stringify(status)), status);
  }
  // A status the native side extends later still decodes to the known fields.
  assert.deepEqual(
    decodeAcceleratorStatus(JSON.stringify({ ...activeStatus, future: 1 })),
    activeStatus,
  );
});
