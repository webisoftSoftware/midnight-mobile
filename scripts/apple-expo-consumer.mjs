import {
  cpSync,
  mkdirSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";

import {
  assertRustUnavailable,
  cleanBuildEnvironment,
  signAndVerify,
} from "./apple-consumer.mjs";
import { fail, REPOSITORY_ROOT, run } from "./apple-native.mjs";

function createExpoConsumer(root) {
  mkdirSync(root, { recursive: true });
  const packageMetadata = {
    name: "midnight-apple-expo-consumer",
    version: "1.0.0",
    private: true,
    main: "index.js",
    dependencies: {
      "@1am/midnight-mobile": "0.1.0-alpha.1",
      expo: "55.0.28",
      "expo-modules-core": "55.0.25",
      react: "19.2.0",
      "react-native": "0.83.6",
    },
  };
  const appMetadata = {
    expo: {
      name: "Midnight Apple Expo Consumer",
      slug: "midnight-apple-expo-consumer",
      version: "1.0.0",
      ios: { bundleIdentifier: "dev.oneam.midnight.consumer" },
    },
  };
  writeFileSync(
    join(root, "package.json"),
    `${JSON.stringify(packageMetadata, null, 2)}\n`,
  );
  writeFileSync(
    join(root, "app.json"),
    `${JSON.stringify(appMetadata, null, 2)}\n`,
  );
  writeFileSync(
    join(root, "index.js"),
    `import { registerRootComponent } from "expo";
import App from "./App";
registerRootComponent(App);
`,
  );
  writeFileSync(
    join(root, "App.js"),
    `import { createMidnightRuntimeApi } from "@1am/midnight-mobile";
import { Text } from "react-native";
export default function App() { void createMidnightRuntimeApi; return <Text>Midnight Apple consumer</Text>; }
`,
  );
}

function findPath(root, predicate) {
  for (const entry of readdirSync(root)) {
    const candidate = join(root, entry);
    const stat = statSync(candidate);
    if (predicate(candidate, stat)) return candidate;
    if (stat.isDirectory()) {
      const nested = findPath(candidate, predicate);
      if (nested !== undefined) return nested;
    }
  }
  return undefined;
}

function workspaceScheme(workspace, environment) {
  const listing = JSON.parse(
    run("xcodebuild", ["-workspace", workspace, "-list", "-json"], {
      env: environment,
    }),
  );
  const expected = "MidnightAppleExpoConsumer";
  if (!listing.workspace?.schemes?.includes(expected)) {
    fail(`Expo workspace has no ${expected} application scheme`);
  }
  return expected;
}

function buildExpoVariant(
  workspace,
  scheme,
  root,
  environment,
  name,
  destination,
) {
  const derivedData = join(root, `derived-${name}`);
  run(
    "xcodebuild",
    [
      "-workspace",
      workspace,
      "-scheme",
      scheme,
      "-configuration",
      "Release",
      "-destination",
      destination,
      "-derivedDataPath",
      derivedData,
      "CODE_SIGNING_ALLOWED=NO",
      "CODE_SIGNING_REQUIRED=NO",
      "build",
    ],
    { env: environment },
  );
  return derivedData;
}

function archiveExpo(workspace, scheme, root, environment) {
  const archive = join(root, "MidnightAppleExpoConsumer.xcarchive");
  run(
    "xcodebuild",
    [
      "-workspace",
      workspace,
      "-scheme",
      scheme,
      "-configuration",
      "Release",
      "-destination",
      "generic/platform=iOS",
      "-archivePath",
      archive,
      "CODE_SIGNING_ALLOWED=NO",
      "CODE_SIGNING_REQUIRED=NO",
      "archive",
    ],
    { env: environment },
  );
  const archivedApp = findPath(
    join(archive, "Products/Applications"),
    (path, stat) => stat.isDirectory() && path.endsWith(".app"),
  );
  if (archivedApp === undefined) fail("Expo archive contains no app");
  signAndVerify(archivedApp, join(root, "signed-archive"), environment);
  return archive;
}

export function validateTarballExpoConsumer(installedPackage, root) {
  createExpoConsumer(root);
  run("/bin/cp", [
    "-cR",
    join(REPOSITORY_ROOT, "node_modules"),
    join(root, "node_modules"),
  ]);
  const packageTarget = join(root, "node_modules/@1am/midnight-mobile");
  rmSync(packageTarget, { force: true, recursive: true });
  cpSync(installedPackage, packageTarget, { recursive: true });
  run(
    process.execPath,
    [
      join(root, "node_modules/expo/bin/cli"),
      "prebuild",
      "--platform",
      "ios",
      "--no-install",
      "--clean",
    ],
    { cwd: root },
  );
  run("pod", ["install", `--project-directory=${join(root, "ios")}`], {
    cwd: root,
  });
  const iosRoot = join(root, "ios");
  const workspaces = readdirSync(iosRoot).filter((entry) => {
    const candidate = join(iosRoot, entry);
    return entry.endsWith(".xcworkspace") && statSync(candidate).isDirectory();
  });
  if (workspaces.length !== 1) {
    fail(`Expo prebuild produced ${String(workspaces.length)} app workspaces`);
  }
  const workspace = join(iosRoot, workspaces[0]);
  const environment = cleanBuildEnvironment(root);
  assertRustUnavailable(environment);
  const scheme = workspaceScheme(workspace, environment);
  const simulatorData = buildExpoVariant(
    workspace,
    scheme,
    root,
    environment,
    "simulator",
    "generic/platform=iOS Simulator",
  );
  const simulatorApp = findPath(
    join(simulatorData, "Build/Products"),
    (path, stat) => stat.isDirectory() && path.endsWith(".app"),
  );
  if (simulatorApp === undefined) fail("Expo simulator build produced no app");
  signAndVerify(simulatorApp, join(root, "signed-simulator"), environment);
  buildExpoVariant(
    workspace,
    scheme,
    root,
    environment,
    "device",
    "generic/platform=iOS",
  );
  const archive = archiveExpo(workspace, scheme, root, environment);
  return { archive, scheme, signing: "ad-hoc strict verification" };
}
