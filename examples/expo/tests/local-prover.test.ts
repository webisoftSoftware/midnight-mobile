import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";

import {
  localProverConfiguration,
  localProverRequests,
  runAcceleratedProve,
  runLocalProverSmokeTest,
} from "../src/local-prover-demo";

function sha256(bytes: Uint8Array): string {
  return createHash("sha256").update(bytes).digest("hex");
}

await test("local prover requests match the deterministic Rust fixtures", () => {
  const requests = localProverRequests();
  assert.equal(requests.check.length, 560);
  assert.equal(
    sha256(requests.check),
    "70bfc0fc2c7e05c38608fe1e336e1b7a38d599a14d02a2bc5bf7a1e96d191fe0",
  );
  assert.equal(requests.prove.length, 578);
  assert.equal(
    sha256(requests.prove),
    "9cfed5466ae1dae73dab40373c81abf94ba15ee1c6c1c3f004156cdb0a61f608",
  );
});

for (const platform of ["ios", "android"] as const) {
  await test(`${platform} local prover config uses bundled artifacts`, () => {
    const configuration = localProverConfiguration(platform);
    const files = [
      configuration.parameters[0]?.file,
      configuration.circuits[0]?.proverKey,
      configuration.circuits[0]?.verifierKey,
      configuration.circuits[0]?.ir,
    ];
    assert.ok(files.every((file) => file !== undefined));
    assert.ok(
      files.every((file) =>
        platform === "android"
          ? file.uri.startsWith("asset://local-prover/")
          : file.uri.startsWith("bundle://LocalProverArtifacts/"),
      ),
    );
    assert.equal(
      configuration.circuits[0]?.keyLocation,
      "midnight/zswap/spend",
    );
    assert.equal(
      configuration.circuits[0].verifierKey.sha256,
      "544554effd7ae9fb9063be52a9ec2a986756301071fcd97bb4598fb45a335658",
    );
  });

  await test(`${platform} local prover does not fake success without native module`, async () => {
    const report = await runLocalProverSmokeTest(platform);
    assert.equal(report.passed, false);
    assert.equal(report.steps.at(-1)?.detail, "NATIVE_INTERNAL");
  });
}

const activeStatus = {
  requested: true,
  availability: "active",
  active: true,
  tripped: false,
  cpuRetries: 0,
  device: "Apple A18 Pro GPU",
  gpuOperations: 0,
  cpuOperations: 0,
  deviceMicros: 0,
  tables: 0,
  tablePoints: 0,
} as const;

function acceleratedProver(
  statuses: readonly (typeof activeStatus | object)[],
) {
  const switched: boolean[] = [];
  let reads = 0;
  let proofs = 0;
  return {
    switched,
    proofs: () => proofs,
    prover: {
      prove: () => {
        proofs += 1;
        return Promise.resolve(new Uint8Array([1, 2, 3]));
      },
      setAccelerator: (enabled: boolean) => {
        switched.push(enabled);
        return Promise.resolve();
      },
      getAcceleratorStatus: () => {
        const status = statuses[Math.min(reads, statuses.length - 1)];
        reads += 1;
        return Promise.resolve({ ...activeStatus, ...status });
      },
    },
  };
}

await test("GPU prove step reports both proofs and switches the accelerator off", async () => {
  const fake = acceleratedProver([{}, { gpuOperations: 144 }]);
  const step = await runAcceleratedProve(fake.prover, new Uint8Array([9]));

  assert.equal(step.ok, true);
  assert.match(
    step.detail,
    /^3 bytes; first \d+ ms, repeat \d+ ms; 144 GPU operations on Apple A18 Pro GPU$/u,
  );
  assert.equal(fake.proofs(), 2);
  assert.deepEqual(fake.switched, [true, false]);
});

await test("GPU prove step passes without proving when no GPU is usable", async () => {
  const fake = acceleratedProver([
    { active: false, availability: "no-device", device: null },
  ]);
  const step = await runAcceleratedProve(fake.prover, new Uint8Array([9]));

  assert.deepEqual(step, {
    name: "run native prove with GPU",
    ok: true,
    detail: "GPU not used: no-device",
  });
  assert.equal(fake.proofs(), 0);
  assert.deepEqual(fake.switched, [true, false]);
});

await test("GPU prove step fails when the proofs did not use the GPU", async () => {
  // No GPU operation was counted: the proofs ran on the CPU.
  const idle = acceleratedProver([{}, {}]);
  assert.equal(
    (await runAcceleratedProve(idle.prover, new Uint8Array([9]))).ok,
    false,
  );
  // The accelerator tripped: a proof was made again on the CPU.
  const tripped = acceleratedProver([
    {},
    { gpuOperations: 72, tripped: true, active: false, cpuRetries: 1 },
  ]);
  assert.equal(
    (await runAcceleratedProve(tripped.prover, new Uint8Array([9]))).ok,
    false,
  );
  assert.deepEqual(tripped.switched, [true, false]);
});

await test("GPU prove step is skipped on a native build without the accelerator", async () => {
  const step = await runAcceleratedProve(
    { prove: () => Promise.resolve(new Uint8Array([1])) },
    new Uint8Array([9]),
  );

  assert.deepEqual(step, {
    name: "run native prove with GPU",
    ok: true,
    detail: "not available in this native build",
  });
});

await test("GPU prove step switches the accelerator off when a proof fails", async () => {
  const fake = acceleratedProver([{}]);
  fake.prover.prove = () => Promise.reject(new Error("PROOF_FAILED"));

  await assert.rejects(() =>
    runAcceleratedProve(fake.prover, new Uint8Array([9])),
  );
  assert.deepEqual(fake.switched, [true, false]);
});
