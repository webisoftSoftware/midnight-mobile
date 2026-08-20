import assert from "node:assert/strict";
import test from "node:test";

import { decodeBase64, encodeBase64 } from "../src/base64.js";
import { MidnightRuntimeError, normalizeMidnightError } from "../src/errors.js";
import {
  InMemoryMidnightCheckpointStore,
  silentMidnightLogger,
} from "../src/host.js";

await test("base64 codec round-trips canonical values", () => {
  const fixtures = [
    new Uint8Array(),
    new Uint8Array([0]),
    new Uint8Array([0, 1]),
    new Uint8Array([0, 1, 2]),
    new Uint8Array([0, 1, 2, 3]),
  ];
  for (const fixture of fixtures) {
    assert.deepEqual(decodeBase64(encodeBase64(fixture)), fixture);
  }
});

await test("base64 decoder rejects non-canonical input", () => {
  for (const value of ["A", "AA=A", "AA?=", "AB==", "AAB=", "===="]) {
    assert.throws(() => decodeBase64(value), {
      code: "INVALID_ARGUMENT",
    });
  }
});

await test("native errors normalize every supported carrier", () => {
  const existing = new MidnightRuntimeError("CANCELLED");
  assert.equal(normalizeMidnightError(existing), existing);
  assert.equal(normalizeMidnightError({ code: "syncGap" }).code, "SYNC_GAP");
  assert.equal(
    normalizeMidnightError({ name: "proofFailed" }).code,
    "PROOF_FAILED",
  );
  assert.equal(
    normalizeMidnightError({ message: "insufficient dust" }).code,
    "INSUFFICIENT_DUST",
  );
  assert.equal(
    normalizeMidnightError("submission-status-unknown").code,
    "SUBMISSION_STATUS_UNKNOWN",
  );
  // A token shortfall, a stale approval, and a transaction the wallet cannot
  // fund each reach JavaScript as themselves, never as NATIVE_INTERNAL.
  assert.equal(
    normalizeMidnightError({ code: "insufficientFunds" }).code,
    "INSUFFICIENT_FUNDS",
  );
  assert.equal(
    normalizeMidnightError({ name: "balanceApprovalChanged" }).code,
    "BALANCE_APPROVAL_CHANGED",
  );
  assert.equal(
    normalizeMidnightError("unsupported-transaction").code,
    "UNSUPPORTED_TRANSACTION",
  );
  assert.equal(normalizeMidnightError(4).code, "NATIVE_INTERNAL");
  assert.equal(
    normalizeMidnightError({ code: "not-real" }).code,
    "NATIVE_INTERNAL",
  );
});

await test("checkpoint store clones on save and load and removes values", async () => {
  const store = new InMemoryMidnightCheckpointStore();
  const original = { version: 1 as const, bytes: new Uint8Array([1, 2]) };
  assert.equal(await store.load("wallet"), null);
  await store.save("wallet", original);
  original.bytes[0] = 9;
  const loaded = await store.load("wallet");
  assert.deepEqual(loaded?.bytes, new Uint8Array([1, 2]));
  assert.ok(loaded);
  loaded.bytes[1] = 8;
  assert.deepEqual((await store.load("wallet"))?.bytes, new Uint8Array([1, 2]));
  await store.remove("wallet");
  assert.equal(await store.load("wallet"), null);
  silentMidnightLogger.write({ level: "debug", event: "session.open" });
});
