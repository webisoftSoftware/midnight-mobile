import assert from "node:assert/strict";
import test from "node:test";

import {
  validatePackageMetadata,
  validatePackEntries,
} from "./check-react-native-package.mjs";

const metadata = {
  name: "@1am/midnight-mobile",
  version: "0.1.0-alpha.1",
  type: "module",
  main: "./dist/index.js",
  types: "./dist/index.d.ts",
  exports: {
    ".": {
      types: "./dist/index.d.ts",
      import: "./dist/index.js",
      default: "./dist/index.js",
    },
    "./package.json": "./package.json",
  },
  files: [
    "README.md",
    "dist",
    "expo-module.config.json",
    "ios/ExpoMidnightNative.podspec",
    "ios/ExpoMidnightNativeModule.swift",
    "ios/build/MidnightNativeRuntime.xcframework",
    "ios/generated",
    "android/build.gradle",
    "android/src/main",
    "android/generated",
  ],
  sideEffects: false,
  peerDependencies: {
    expo: ">=55.0.0 <56.0.0",
    "expo-modules-core": ">=55.0.0 <56.0.0",
    react: ">=19.2.0 <20.0.0",
    "react-native": ">=0.83.0 <0.84.0",
  },
  devDependencies: {
    "@types/node": "24.10.1",
    "@types/react": "19.2.14",
    expo: "55.0.28",
    "expo-modules-core": "55.0.25",
    react: "19.2.0",
    "react-native": "0.83.6",
    typescript: "5.9.3",
  },
  engines: { node: ">=22 <23" },
};
const autolinking = {
  platforms: ["apple", "android"],
  apple: { modules: ["ExpoMidnightNativeModule"] },
  android: {
    modules: ["expo.modules.midnightnative.ExpoMidnightNativeModule"],
  },
};

await test("package metadata enforces the M4 consumer contract", () => {
  assert.deepEqual(validatePackageMetadata(metadata, autolinking), []);
  assert.match(
    validatePackageMetadata(
      { ...metadata, dependencies: { internal: "1.0.0" } },
      autolinking,
    )[0] ?? "",
    /dependencies must be absent/u,
  );
  assert.match(
    validatePackageMetadata(
      {
        ...metadata,
        peerDependencies: { ...metadata.peerDependencies, expo: "*" },
      },
      autolinking,
    )[0] ?? "",
    /peerDependencies/u,
  );
  assert.match(
    validatePackageMetadata(
      { ...metadata, scripts: { postinstall: "download-native-binaries" } },
      autolinking,
    ).join("\n"),
    /scripts\.postinstall must be absent/u,
  );
});

await test("tarball inspection requires and allowlists M4 native binaries", () => {
  const required = [
    "README.md",
    "android/build.gradle",
    "android/generated/README.md",
    "android/src/main/jniLibs/arm64-v8a/libmidnight_native_runtime.so",
    "android/src/main/jniLibs/x86_64/libmidnight_native_runtime.so",
    "android/src/main/java/expo/modules/midnightnative/ExpoMidnightNativeModule.kt",
    "dist/index.d.ts",
    "dist/index.js",
    "expo-module.config.json",
    "ios/ExpoMidnightNative.podspec",
    "ios/ExpoMidnightNativeModule.swift",
    "ios/build/MidnightNativeRuntime.xcframework/Info.plist",
    "ios/generated/README.md",
    "package.json",
  ].map((path) => ({ path }));
  assert.deepEqual(validatePackEntries(required), []);
  const errors = validatePackEntries([
    ...required,
    { path: "src/private.ts" },
    {
      path: "ios/build/MidnightNativeRuntime.xcframework/slice/unsupported.dylib",
    },
  ]);
  assert.equal(
    errors.some((error) => error.includes("non-allowlisted")),
    true,
  );
  assert.equal(
    errors.some((error) => error.includes("unsupported native binary")),
    true,
  );
});
