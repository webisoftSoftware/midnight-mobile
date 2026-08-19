import assert from "node:assert/strict";
import test from "node:test";

import {
  canonicalXcframeworkPlist,
  conciseFailureDetails,
  FRAMEWORK_BUNDLE,
  FRAMEWORK_NAME,
  XCODE_BUILD_CONCURRENCY_ARGUMENTS,
} from "./apple-native.mjs";

await test("XCFramework metadata has a canonical library order", () => {
  const plist = canonicalXcframeworkPlist();
  const device = plist.indexOf("<string>ios-arm64</string>");
  const simulator = plist.indexOf(
    "<string>ios-arm64_x86_64-simulator</string>",
  );

  assert.ok(device >= 0);
  assert.ok(simulator > device);
  assert.equal(
    plist.match(new RegExp(`${FRAMEWORK_BUNDLE}/${FRAMEWORK_NAME}`, "gu"))
      ?.length,
    2,
  );
  assert.equal(plist, canonicalXcframeworkPlist());
});

await test("Xcode builds use bounded parallelism on shared runners", () => {
  assert.deepEqual(XCODE_BUILD_CONCURRENCY_ARGUMENTS, ["-jobs", "2"]);
});

await test("failure output preserves compiler diagnostics before a long tail", () => {
  const details = [
    "prefix".repeat(3_000),
    "/tmp/Module.swift:7:12: error: synthetic compiler failure",
    "SwiftCompile " + "source.swift ".repeat(2_000),
  ].join("\n");
  const concise = conciseFailureDetails(details);

  assert.match(concise, /Extracted diagnostics/u);
  assert.match(concise, /synthetic compiler failure/u);
  assert.match(concise, /Output tail/u);
  assert.ok(concise.length < details.length);
});
