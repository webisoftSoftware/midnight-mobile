import assert from "node:assert/strict";
import test from "node:test";

import {
  canonicalXcframeworkPlist,
  FRAMEWORK_BUNDLE,
  FRAMEWORK_NAME,
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
