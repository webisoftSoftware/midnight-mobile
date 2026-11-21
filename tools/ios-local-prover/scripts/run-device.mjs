// Runs the local-prover validation application on a connected iPhone and reads
// back its report: a Zswap spend proved on the CPU and, where the phone has a
// usable GPU, proved again with the GPU accelerator on.
//
// Run the simulator harness first. This script reuses what that run leaves in
// place: the Expo project under target/ios-local-prover/consumer, the staged
// prover artifacts, and the XCFramework inside the package.
//
// Environment:
//   IOS_PROVER_DEVICE_ID         identifier, UDID or name of the iPhone, as
//                                `xcrun devicectl list devices` prints it
//   IOS_PROVER_DEVELOPMENT_TEAM  Apple developer team that signs the application
//   IOS_PROVER_REQUIRE_GPU=1     fail unless the GPU made the accelerated proofs
//
// The phone must be unlocked, trust this Mac, and have Developer Mode on.
// Xcode signs the application automatically, which registers the validation
// bundle identifier with the team. Xcode must be signed in to that team.
//
// A phone that is newer than the installed Xcode has no developer services:
// Xcode cannot install on it, start an application or read its files. The
// script then installs with `ideviceinstaller`, asks for the application to be
// opened by hand, and reads the results with `afcclient` (both from
// libimobiledevice, over the cable). Such a phone must already be registered
// with the team, because Xcode cannot register it.

import { spawnSync } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { applicationSource, localProverConfiguration } from "./app-source.mjs";
import { findPath, run, sha256 } from "./harness-utils.mjs";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(scriptDirectory, "../../..");
const targetRoot = resolve(repositoryRoot, "target/ios-local-prover");
const applicationRoot = resolve(targetRoot, "consumer");
const artifactsRoot = resolve(
  repositoryRoot,
  "target/android-prover-spike/artifacts",
);
const resultsRoot = resolve(targetRoot, "device-results");
const bundleIdentifier = "dev.oneam.midnightmobile.localprover.validation";
const timeoutMillis = 10 * 60 * 1_000;

const device = process.env.IOS_PROVER_DEVICE_ID;
if (device === undefined || device.length === 0) {
  throw new Error("IOS_PROVER_DEVICE_ID is required");
}
const developmentTeam = process.env.IOS_PROVER_DEVELOPMENT_TEAM;
if (developmentTeam === undefined || developmentTeam.length === 0) {
  throw new Error("IOS_PROVER_DEVELOPMENT_TEAM is required");
}

function deviceDetails() {
  const listing = resolve(targetRoot, "devices.json");
  run("xcrun", ["devicectl", "list", "devices", "--json-output", listing]);
  const devices = JSON.parse(readFileSync(listing, "utf8")).result?.devices;
  const match = (devices ?? []).find((candidate) =>
    [
      candidate.identifier,
      candidate.hardwareProperties?.udid,
      candidate.deviceProperties?.name,
    ].includes(device),
  );
  if (match === undefined) {
    throw new Error(`devicectl does not know device ${device}`);
  }
  return {
    identifier: match.identifier,
    udid: match.hardwareProperties?.udid,
    name: match.deviceProperties?.name,
    model: match.hardwareProperties?.marketingName,
    productType: match.hardwareProperties?.productType,
    osVersion: match.deviceProperties?.osVersionNumber,
    // Whether Xcode can install on, start and read from this phone.
    developerServices: match.deviceProperties?.ddiServicesAvailable === true,
  };
}

// Writes the application's source again, so that a project an earlier run left
// in place runs the current application.
function writeApplicationSource() {
  const manifest = JSON.parse(
    readFileSync(resolve(artifactsRoot, "artifact-manifest.json"), "utf8"),
  );
  writeFileSync(
    resolve(applicationRoot, "App.js"),
    applicationSource({
      configuration: localProverConfiguration(manifest),
      checkRequest: readFileSync(
        resolve(artifactsRoot, manifest.checkRequest.path),
      ),
      proveRequest: readFileSync(resolve(artifactsRoot, manifest.request.path)),
    }),
  );
}

function buildApplication(details) {
  const iosRoot = resolve(applicationRoot, "ios");
  const workspace = existsSync(iosRoot)
    ? readdirSync(iosRoot).find((entry) => entry.endsWith(".xcworkspace"))
    : undefined;
  if (workspace === undefined) {
    throw new Error(
      "no application project is in place: run the simulator harness first",
    );
  }
  writeApplicationSource();
  const scheme = workspace.replace(/\.xcworkspace$/u, "");
  const derivedData = resolve(targetRoot, "derived-data-device");
  run(
    "xcodebuild",
    [
      "-workspace",
      resolve(iosRoot, workspace),
      "-scheme",
      scheme,
      "-configuration",
      "Release",
      "-jobs",
      "2",
      "-quiet",
      "-destination",
      // Xcode refuses a phone without developer services as a destination.
      details.developerServices
        ? `id=${details.identifier}`
        : "generic/platform=iOS",
      "-derivedDataPath",
      derivedData,
      "-allowProvisioningUpdates",
      "-allowProvisioningDeviceRegistration",
      `DEVELOPMENT_TEAM=${developmentTeam}`,
      "CODE_SIGN_STYLE=Automatic",
      "build",
    ],
    {
      cwd: applicationRoot,
      env: { ...process.env, NODE_BINARY: process.execPath },
      inherit: true,
    },
  );
  const app = findPath(
    resolve(derivedData, "Build/Products/Release-iphoneos"),
    (path, stat) => stat.isDirectory() && path.endsWith(".app"),
  );
  if (app === undefined) throw new Error("xcodebuild produced no device app");
  const profile = run("security", [
    "cms",
    "-D",
    "-i",
    resolve(app, "embedded.mobileprovision"),
  ]);
  if (details.udid !== undefined && !profile.includes(details.udid)) {
    throw new Error(
      `the signing profile does not include this phone: register device ` +
        `${details.udid} with team ${developmentTeam}, then run again`,
    );
  }
  return app;
}

// The prover artifacts are added after the build, so the application bundle is
// signed again with the identity and entitlements that Xcode gave it.
function stageArtifactsAndSign(app) {
  cpSync(artifactsRoot, resolve(app, "LocalProverArtifacts"), {
    recursive: true,
  });
  const described = spawnSync("codesign", ["-dvv", app], { encoding: "utf8" });
  const identity = /^Authority=(.+)$/mu.exec(described.stderr ?? "")?.[1];
  if (identity === undefined) {
    throw new Error("the built application has no signing identity");
  }
  const entitlements = resolve(targetRoot, "device-entitlements.plist");
  writeFileSync(
    entitlements,
    run("codesign", ["-d", "--entitlements", "-", "--xml", app]),
  );
  run("codesign", [
    "--force",
    "--sign",
    identity,
    "--entitlements",
    entitlements,
    "--timestamp=none",
    app,
  ]);
  run("codesign", ["--verify", "--strict", "--verbose=2", app]);
}

// Installs the application and starts it. Without developer services the
// application cannot be started from here and is opened by hand.
function installAndLaunch(app, details) {
  // An earlier installation is removed first: its Documents directory would
  // otherwise still hold the previous run's result.
  if (!details.developerServices) {
    spawnSync("ideviceinstaller", [
      "-u",
      details.udid,
      "uninstall",
      bundleIdentifier,
    ]);
    run("ideviceinstaller", ["-u", details.udid, "install", app], {
      inherit: true,
    });
    console.log(
      'Open the application "Midnight iOS Local Prover Validation" on the ' +
        "phone now and keep the phone unlocked.",
    );
    return;
  }
  const target = ["-d", details.identifier];
  spawnSync("xcrun", [
    "devicectl",
    "device",
    "uninstall",
    "app",
    ...target,
    bundleIdentifier,
  ]);
  run("xcrun", ["devicectl", "device", "install", "app", ...target, app]);
  run("xcrun", [
    "devicectl",
    "device",
    "process",
    "launch",
    ...target,
    "--terminate-existing",
    bundleIdentifier,
  ]);
}

// Copies one file out of the application's Documents directory. `false`: the
// file is not there (yet), or the phone is locked.
function copyFromDevice(details, name) {
  const destination = resolve(resultsRoot, name);
  rmSync(destination, { force: true });
  const [command, argumentsList] = details.developerServices
    ? [
        "xcrun",
        [
          "devicectl",
          "device",
          "copy",
          "from",
          "-d",
          details.identifier,
          "--domain-type",
          "appDataContainer",
          "--domain-identifier",
          bundleIdentifier,
          "--source",
          `Documents/${name}`,
          "--destination",
          destination,
        ],
      ]
    : [
        "afcclient",
        [
          "-u",
          details.udid,
          "--container",
          bundleIdentifier,
          "get",
          `/Documents/${name}`,
          destination,
        ],
      ];
  const copied = spawnSync(command, argumentsList, { encoding: "utf8" });
  return copied.status === 0 && existsSync(destination);
}

function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
}

async function waitForResult(details) {
  const started = Date.now();
  while (Date.now() - started < timeoutMillis) {
    if (copyFromDevice(details, "local-prover-result.json")) {
      try {
        return JSON.parse(
          readFileSync(
            resolve(resultsRoot, "local-prover-result.json"),
            "utf8",
          ),
        );
      } catch {
        // The application writes this small completion marker last; retry a partial read.
      }
    }
    await delay(2_000);
  }
  throw new Error("device local prover timed out; is the phone unlocked?");
}

// Every proof the application made goes through the host validators.
function hostValidate(details, accelerated) {
  const outputs = [
    ["check-response.bin", "validate_android_prover_check"],
    ["proof-v2.bin", "validate_android_prover_proof"],
  ];
  if (accelerated) {
    outputs.push(["proof-v2-accelerated.bin", "validate_android_prover_proof"]);
  }
  for (const [name, example] of outputs) {
    if (!copyFromDevice(details, name)) {
      throw new Error(`the application result is missing ${name}`);
    }
    run(
      "cargo",
      [
        "run",
        "--offline",
        "--locked",
        "--quiet",
        "--package",
        "midnight-mobile-runtime",
        "--example",
        example,
      ],
      { input: readFileSync(resolve(resultsRoot, name)) },
    );
  }
  return Object.fromEntries(
    outputs.map(([name]) => [name, sha256(resolve(resultsRoot, name))]),
  );
}

function millis(value) {
  return typeof value === "number" ? `${String(Math.round(value))} ms` : "n/a";
}

async function main() {
  mkdirSync(resultsRoot, { recursive: true });
  const details = deviceDetails();
  const app = buildApplication(details);
  stageArtifactsAndSign(app);
  // A result an earlier run left on the phone must not be read as this run's.
  rmSync(resolve(resultsRoot, "local-prover-result.json"), { force: true });
  installAndLaunch(app, details);
  const result = await waitForResult(details);
  if (result.success !== true) {
    throw new Error(`device prover failed: ${JSON.stringify(result)}`);
  }
  const accelerator = result.accelerator ?? {};
  const used = accelerator.active === true && accelerator.gpuOperations > 0;
  if (!used && process.env.IOS_PROVER_REQUIRE_GPU === "1") {
    throw new Error(`the GPU was not used: ${JSON.stringify(accelerator)}`);
  }
  if (used && (accelerator.tripped === true || accelerator.cpuRetries !== 0)) {
    throw new Error(`the GPU prover fell back: ${JSON.stringify(accelerator)}`);
  }
  const report = {
    schemaVersion: 1,
    capturedAt: new Date().toISOString(),
    device: details,
    bundleIdentifier,
    nativeResult: result,
    outputs: hostValidate(details, used),
  };
  writeFileSync(
    resolve(targetRoot, "device-run-report.json"),
    `${JSON.stringify(report, null, 2)}\n`,
  );
  console.log(
    `iOS device prover passed on ${String(details.model)} ` +
      `(iOS ${String(details.osVersion)})`,
  );
  console.log(
    `CPU: first ${millis(result.proveMillis)}, ` +
      `repeat ${millis(result.warmProveMillis)}`,
  );
  console.log(
    used
      ? `GPU: first ${millis(accelerator.coldProveMillis)}, ` +
          `repeat ${millis(accelerator.warmProveMillis)}, ` +
          `${String(accelerator.gpuOperations)} GPU operations on ` +
          String(accelerator.device)
      : `GPU not used: ${JSON.stringify(accelerator)}`,
  );
}

await main();
