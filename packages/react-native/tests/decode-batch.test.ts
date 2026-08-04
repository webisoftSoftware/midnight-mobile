import assert from "node:assert/strict";
import test from "node:test";

import { decodeOperationStep } from "../src/decode.js";
import { MidnightRuntimeError } from "../src/errors.js";

await test("network steps decode an additive batched effects list and reject malformed ones", () => {
  const handle = { id: 4, generation: 5 };
  const base = {
    kind: "network",
    operation: handle,
    effectId: "5:4:1#0",
    effect: "check",
    endpointRole: "proof",
    bodyBase64: "AQ==",
  };
  const effect = (effectId: string, effect: string) => ({
    effectId,
    effect,
    endpointRole: "proof",
    bodyBase64: "AQ==",
  });

  // Absent: a single-effect round stays wire-identical to the pre-batching protocol.
  const single = decodeOperationStep("transfer", base);
  assert.equal(single.kind, "network");
  assert.equal("effects" in single, false);

  const batched = decodeOperationStep("transfer", {
    ...base,
    effects: [effect("5:4:1#0", "check"), effect("5:4:1#1", "prove")],
  });
  assert.equal(batched.kind, "network");
  assert.deepEqual(
    batched.effects?.map((entry) => [entry.effectId, entry.effect]),
    [
      ["5:4:1#0", "check"],
      ["5:4:1#1", "prove"],
    ],
  );

  // Present but malformed must fail closed rather than silently downgrade to the
  // first effect, which would strand the runtime waiting on unexecuted requests.
  for (const [label, effects] of [
    ["not an array", {}],
    ["a one-element list", [effect("5:4:1#0", "check")]],
    [
      "a non-proof effect",
      [effect("5:4:1#0", "sync"), effect("5:4:1#1", "prove")],
    ],
    [
      "a non-proof endpoint role",
      [
        { ...effect("5:4:1#0", "check"), endpointRole: "indexer" },
        effect("5:4:1#1", "prove"),
      ],
    ],
    [
      "a malformed body",
      [
        { ...effect("5:4:1#0", "check"), bodyBase64: "!!" },
        effect("5:4:1#1", "prove"),
      ],
    ],
  ] as const) {
    assert.throws(
      () => decodeOperationStep("transfer", { ...base, effects }),
      MidnightRuntimeError,
      `${label} must be rejected`,
    );
  }
});
