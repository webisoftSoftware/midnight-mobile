import assert from "node:assert/strict";
import test from "node:test";

import {
  EXPECTED_ABI,
  assertExactGeneratedAbi,
  extractGeneratedAbi,
  normalizeGeneratedText,
} from "./check-uniffi-bindings.mjs";

function camelCase(value) {
  return value.replaceAll(/_([a-z])/gu, (_match, letter) =>
    letter.toUpperCase(),
  );
}

function generatedFixtures(names = EXPECTED_ABI) {
  return {
    swift: names
      .map((name) => `public func ${camelCase(name)}()throws {}`)
      .join("\n"),
    kotlin: names
      .map(
        (name) =>
          `@Throws(MidnightRuntimeException::class) fun \`${camelCase(name)}\`()`,
      )
      .join("\n"),
    header: names
      .map(
        (name) => `void uniffi_midnight_native_runtime_fn_func_${name}(void);`,
      )
      .join("\n"),
  };
}

await test("accepts exactly the reviewed eight-function ABI", () => {
  const fixture = generatedFixtures();
  assert.doesNotThrow(() =>
    assertExactGeneratedAbi(fixture.swift, fixture.kotlin, fixture.header),
  );
  assert.deepEqual(
    extractGeneratedAbi(fixture.swift, fixture.kotlin, fixture.header)
      .headerNames,
    EXPECTED_ABI,
  );
});

await test("rejects a missing generated function", () => {
  const fixture = generatedFixtures(EXPECTED_ABI.slice(1));
  assert.throws(
    () =>
      assertExactGeneratedAbi(fixture.swift, fixture.kotlin, fixture.header),
    /ABI changed/u,
  );
});

await test("rejects an additional generated function", () => {
  const fixture = generatedFixtures([...EXPECTED_ABI, "removed_capability"]);
  assert.throws(
    () =>
      assertExactGeneratedAbi(fixture.swift, fixture.kotlin, fixture.header),
    /ABI changed/u,
  );
});

await test("rejects duplicate generated declarations", () => {
  const fixture = generatedFixtures([...EXPECTED_ABI, EXPECTED_ABI[0]]);
  assert.throws(
    () =>
      assertExactGeneratedAbi(fixture.swift, fixture.kotlin, fixture.header),
    /duplicate function declarations/u,
  );
});

await test("normalizes generated text deterministically", () => {
  const unnormalized = "first \t\r\nsecond\t\r\n \t\r\n\r\n";
  const normalized = "first\nsecond\n";
  assert.equal(normalizeGeneratedText(unnormalized), normalized);
  assert.equal(normalizeGeneratedText(normalized), normalized);
});
