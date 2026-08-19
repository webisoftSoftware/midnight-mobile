import assert from "node:assert/strict";
import test from "node:test";

import { podspecModuleContractError } from "../native/apple-consumer.mjs";
import {
  validatePackageMetadata,
  validatePackEntries,
} from "./check-react-native-package.mjs";

const metadata = {
  name: "@1am/midnight-mobile",
  version: "0.1.0-alpha.1",
  license: "MIT",
  type: "module",
  main: "./dist/index.js",
  types: "./dist/index.d.ts",
  exports: {
    ".": {
      types: "./dist/index.d.ts",
      import: "./dist/index.js",
      default: "./dist/index.js",
    },
    "./local-prover": {
      types: "./dist/local-prover.d.ts",
      import: "./dist/local-prover.js",
      default: "./dist/local-prover.js",
    },
    "./package.json": "./package.json",
  },
  files: [
    "README.md",
    "dist",
    "expo-module.config.json",
    "ios/MidnightMobileRuntime.podspec",
    "ios/MidnightMobileLocalProverModule.swift",
    "ios/MidnightMobileLocalProverArtifacts.swift",
    "ios/MidnightMobileRuntimeModule.swift",
    "ios/MidnightMobileLocalProverFFI.h",
    "ios/build/MidnightMobileRuntime.xcframework",
    "ios/generated",
    "android/build.gradle",
    "android/consumer-rules.pro",
    "android/local-prover",
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
  engines: { node: ">=22 <23 || >=24 <25" },
};
const autolinking = {
  platforms: ["apple", "android"],
  apple: {
    swiftModuleName: "MidnightMobileExpo",
    modules: ["MidnightMobileRuntimeModule", "MidnightMobileLocalProverModule"],
  },
  android: {
    modules: [
      "dev.oneam.midnightmobile.MidnightMobileRuntimeModule",
      "dev.oneam.midnightmobile.localprover.MidnightMobileLocalProverModule",
    ],
  },
};

await test("CocoaPods keeps the Expo target module distinct from the Rust framework", () => {
  assert.equal(
    podspecModuleContractError({
      name: "MidnightMobileRuntime",
      module_name: "MidnightMobileExpo",
    }),
    undefined,
  );
  assert.match(
    podspecModuleContractError({
      name: "MidnightMobileRuntime",
      module_name: "MidnightMobileRuntime",
    }) ?? "",
    /distinct MidnightMobileRuntime framework module/u,
  );
});

await test("Expo autolinking imports the Expo pod target module", () => {
  assert.deepEqual(validatePackageMetadata(metadata, autolinking), []);
  const withoutSwiftModule = {
    ...autolinking,
    apple: { modules: autolinking.apple.modules },
  };
  assert.match(
    validatePackageMetadata(metadata, withoutSwiftModule).join("\n"),
    /Expo autolinking metadata/u,
  );
});

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
  assert.match(
    validatePackageMetadata(
      { ...metadata, engines: { node: ">=22" } },
      autolinking,
    ).join("\n"),
    /engines/u,
  );
});

await test("package metadata requires the MIT license", () => {
  for (const license of [undefined, "MIT OR Apache-2.0", "Apache-2.0"]) {
    assert.match(
      validatePackageMetadata({ ...metadata, license }, autolinking).join("\n"),
      /license/u,
    );
  }
  assert.deepEqual(validatePackageMetadata(metadata, autolinking), []);
});

await test("tarball inspection requires and allowlists M4 native binaries", () => {
  const required = [
    "README.md",
    "android/build.gradle",
    "android/consumer-rules.pro",
    "android/generated/README.md",
    "android/local-prover/dev/oneam/midnightmobile/localprover/LocalProverBridge.kt",
    "android/src/main/jniLibs/arm64-v8a/libmidnight_mobile_runtime.so",
    "android/src/main/jniLibs/x86_64/libmidnight_mobile_runtime.so",
    "android/src/main/java/dev/oneam/midnightmobile/MidnightMobileRuntimeModule.kt",
    "android/src/main/java/dev/oneam/midnightmobile/localprover/MidnightMobileLocalProverModule.kt",
    "dist/index.d.ts",
    "dist/index.js",
    "dist/local-prover.d.ts",
    "dist/local-prover.js",
    "expo-module.config.json",
    "ios/MidnightMobileRuntime.podspec",
    "ios/MidnightMobileLocalProverModule.swift",
    "ios/MidnightMobileLocalProverArtifacts.swift",
    "ios/MidnightMobileRuntimeModule.swift",
    "ios/MidnightMobileLocalProverFFI.h",
    "ios/build/MidnightMobileRuntime.xcframework/Info.plist",
    "ios/generated/README.md",
    "package.json",
  ].map((path) => ({ path }));
  assert.deepEqual(validatePackEntries(required), []);
  const errors = validatePackEntries([
    ...required,
    { path: "src/private.ts" },
    {
      path: "ios/build/MidnightMobileRuntime.xcframework/slice/unsupported.dylib",
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
