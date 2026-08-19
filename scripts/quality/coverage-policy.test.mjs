import assert from "node:assert/strict";
import test from "node:test";

import { loadCoveragePolicy } from "./coverage-policy.mjs";

test("coverage policy keeps permanent TypeScript and Rust thresholds", () => {
  const policy = loadCoveragePolicy();
  assert.deepEqual(policy.typescript.minimumPercent, {
    lines: 85,
    branches: 75,
    functions: 85,
  });
  assert.equal(policy.rust.minimumPercent, 80);
  assert.equal(policy.rust.baselinePercent, 81.31);
  assert.equal(policy.rust.enforcedPercent, 81.31);
  assert.match(policy.rust.ignoreFilenameRegex, /tools\/bindgen/u);
});

test("coverage policy cannot be mutated by callers", () => {
  const policy = loadCoveragePolicy();
  assert.throws(() => {
    policy.typescript.minimumPercent.lines = 0;
  }, TypeError);
  assert.throws(() => {
    policy.rust.enforcedPercent = 0;
  }, TypeError);
});
