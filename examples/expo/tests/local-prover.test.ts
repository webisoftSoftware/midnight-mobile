import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";

import {
  localProverConfiguration,
  localProverRequests,
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
