import assert from "node:assert/strict";
import test from "node:test";

import {
  MIDNIGHT_COMMAND_KINDS,
  type MidnightCommandMap,
  type MidnightCommandResultMap,
} from "../src/commands.js";
import { decodeCommandResult, decodeOperationStep } from "../src/decode.js";
import { PUBLIC_COMMAND_TYPE_FIXTURES } from "../type-tests/public-command-types.js";

const commandAndResultKeysMatch: Readonly<
  Record<keyof MidnightCommandResultMap, true>
> = {
  signData: true,
  createCheckPayload: true,
  parseCheckResult: true,
  createProvingPayload: true,
  canonicalizeTransaction: true,
  createSyncRequest: true,
  deriveShieldedMintContext: true,
  watchShieldedMint: true,
  createShieldedSpentRequest: true,
  applyShieldedSpentResponse: true,
  setShieldedProtocolVersion: true,
  createDustSpendRequest: true,
  createDustCommitmentRequest: true,
  applyDustSpendResolution: true,
  transfer: true,
  dappTransfer: true,
  dappIntent: true,
  generateDust: true,
  previewBalance: true,
  balanceUnsealed: true,
  balanceSealed: true,
  finalizeUnprovenTransaction: true,
  submitFinalized: true,
} satisfies Readonly<Record<keyof MidnightCommandMap, true>>;

await test("command and result maps contain the exact public command set", () => {
  assert.equal(Object.keys(commandAndResultKeysMatch).length, 23);
  assert.equal(MIDNIGHT_COMMAND_KINDS.length, 23);
  assert.deepEqual(
    Object.keys(PUBLIC_COMMAND_TYPE_FIXTURES),
    MIDNIGHT_COMMAND_KINDS,
  );
  assert.equal(new Set(MIDNIGHT_COMMAND_KINDS).size, 23);
  assert.deepEqual(MIDNIGHT_COMMAND_KINDS.slice(0, 5), [
    "signData",
    "createCheckPayload",
    "parseCheckResult",
    "createProvingPayload",
    "canonicalizeTransaction",
  ]);
  assert.deepEqual(MIDNIGHT_COMMAND_KINDS.slice(-3), [
    "balanceSealed",
    "finalizeUnprovenTransaction",
    "submitFinalized",
  ]);
});

await test("strict result decoding preserves command correlation", () => {
  const result = decodeCommandResult("signData", {
    signatureHex: "11".repeat(64),
    verifyingKeyHex: "22".repeat(32),
  });
  assert.equal(result.signatureHex, "11".repeat(64));

  const step = decodeOperationStep("parseCheckResult", {
    kind: "complete",
    operation: null,
    result: { values: [null, "18446744073709551615"] },
  });
  assert.equal(step.kind, "complete");
  assert.deepEqual(step.result.values, [null, "18446744073709551615"]);
});

await test("strict decoders reject malformed native values", () => {
  assert.throws(
    () =>
      decodeCommandResult("signData", {
        signatureHex: "not-hex",
        verifyingKeyHex: "22".repeat(32),
      }),
    { message: "NATIVE_INTERNAL" },
  );
  assert.throws(
    () =>
      decodeOperationStep("signData", {
        kind: "network",
        operation: { id: 1, generation: 1 },
        effectId: "1",
        effect: "prove",
        endpointRole: "unsupported",
        bodyBase64: "",
      }),
    { message: "NATIVE_INTERNAL" },
  );
});
