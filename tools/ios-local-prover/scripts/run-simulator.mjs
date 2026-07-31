import { spawnSync } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { removeTree } from "../../../scripts/quality-utils.mjs";
import { findPath, run, sha256, treeBytes } from "./harness-utils.mjs";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(scriptDirectory, "../../..");
const targetRoot = resolve(repositoryRoot, "target/ios-local-prover");
const applicationRoot = resolve(targetRoot, "consumer");
const artifactsRoot = resolve(
  repositoryRoot,
  "target/android-prover-spike/artifacts",
);
const packageRoot = resolve(repositoryRoot, "packages/react-native");
const xcframework = resolve(
  packageRoot,
  "ios/build/MidnightNativeRuntime.xcframework",
);
const xcframeworkArchive = resolve(
  repositoryRoot,
  "artifacts/apple/MidnightNativeRuntime.xcframework.zip",
);
const device =
  process.env.IOS_PROVER_SIMULATOR_UDID ??
  "BF591F85-D9AF-4F7E-B82D-E4A368EB3D17";
const bundleIdentifier = "dev.oneam.midnight.localprover.validation";
const timeoutMillis = 15 * 60 * 1_000;
const resumeApplicationBuild = process.env.IOS_PROVER_RESUME === "1";

function simulatorDetails() {
  const listing = JSON.parse(
    run("xcrun", ["simctl", "list", "devices", "--json"]),
  );
  for (const [runtime, devices] of Object.entries(listing.devices ?? {})) {
    const match = devices.find((candidate) => candidate.udid === device);
    if (match !== undefined) {
      return {
        udid: device,
        name: match.name,
        state: match.state,
        runtimeIdentifier: runtime,
        architecture: run("uname", ["-m"]).trim(),
      };
    }
  }
  throw new Error(`simctl does not know simulator ${device}`);
}

function prepareArtifacts() {
  const manifest = resolve(artifactsRoot, "artifact-manifest.json");
  if (!existsSync(manifest)) {
    run(
      "node",
      [
        resolve(
          repositoryRoot,
          "tools/android-prover-spike/scripts/prepare-artifacts.mjs",
        ),
      ],
      { inherit: true },
    );
  }
  const parsed = JSON.parse(readFileSync(manifest, "utf8"));
  for (const artifact of parsed.artifacts) {
    const path = resolve(artifactsRoot, artifact.path);
    if (
      !existsSync(path) ||
      statSync(path).size !== artifact.size ||
      sha256(path) !== artifact.sha256
    ) {
      throw new Error(`staged artifact failed verification: ${artifact.path}`);
    }
  }
  for (const request of [parsed.request, parsed.checkRequest]) {
    const path = resolve(artifactsRoot, request.path);
    if (!existsSync(path) || sha256(path) !== request.sha256) {
      throw new Error(`staged request failed verification: ${request.path}`);
    }
  }
  return parsed;
}

function localProverConfiguration(manifest) {
  const byPath = new Map(
    manifest.artifacts.map((artifact) => [artifact.path, artifact]),
  );
  const file = (path) => {
    const artifact = byPath.get(path);
    if (artifact === undefined) throw new Error(`manifest is missing ${path}`);
    return {
      uri: `bundle://LocalProverArtifacts/${path}`,
      size: artifact.size,
      sha256: artifact.sha256,
    };
  };
  return {
    parameters: [{ k: 15, file: file("bls_midnight_2p15") }],
    circuits: [
      {
        keyLocation: "midnight/zswap/spend",
        proverKey: file("zswap/9/spend.prover"),
        verifierKey: file("zswap/9/spend.verifier"),
        ir: file("zswap/9/spend.bzkir"),
      },
    ],
  };
}

function applicationSource(manifest) {
  const request = [
    ...readFileSync(resolve(artifactsRoot, manifest.request.path)),
  ];
  const checkRequest = [
    ...readFileSync(resolve(artifactsRoot, manifest.checkRequest.path)),
  ];
  return `import { createMidnightLocalProver } from "@1am/midnight-mobile/local-prover";
import { File, Paths } from "expo-file-system";
import { useEffect, useState } from "react";
import { Text, View } from "react-native";

const configuration = ${JSON.stringify(localProverConfiguration(manifest))};
const checkRequest = new Uint8Array(${JSON.stringify(checkRequest)});
const proveRequest = new Uint8Array(${JSON.stringify(request)});

function write(name, content) {
  const file = new File(Paths.document, name);
  file.create({ overwrite: true });
  file.write(content);
}

export default function App() {
  const [status, setStatus] = useState("Starting local prover…");
  useEffect(() => {
    void (async () => {
      const startedAt = performance.now();
      let prover;
      let report;
      try {
        prover = await createMidnightLocalProver(configuration);
        const configuredAt = performance.now();
        const checked = await prover.check(checkRequest);
        const checkedAt = performance.now();
        const proof = await prover.prove(proveRequest);
        const provedAt = performance.now();
        write("check-response.bin", checked);
        write("proof-v2.bin", proof);
        report = {
          success: true,
          configureMillis: configuredAt - startedAt,
          checkMillis: checkedAt - configuredAt,
          proveMillis: provedAt - checkedAt,
          checkBytes: checked.length,
          proofBytes: proof.length,
        };
        setStatus("Local check and proof succeeded");
      } catch (error) {
        report = {
          success: false,
          code: typeof error === "object" && error !== null && "code" in error
            ? String(error.code)
            : "UNEXPECTED_ERROR",
        };
        setStatus("Local prover failed");
      } finally {
        if (prover !== undefined) {
          try { await prover.close(); } catch { report.closeFailed = true; }
        }
        write("local-prover-result.json", JSON.stringify(report));
      }
    })();
  }, []);
  return <View><Text testID="local-prover-status">{status}</Text></View>;
}
`;
}

function createConsumer(manifest) {
  removeTree(applicationRoot);
  mkdirSync(applicationRoot, { recursive: true });
  writeFileSync(
    resolve(applicationRoot, "package.json"),
    `${JSON.stringify(
      {
        name: "midnight-ios-local-prover-validation",
        version: "1.0.0",
        private: true,
        main: "index.js",
        dependencies: {
          "@1am/midnight-mobile": "0.1.0-alpha.1",
          expo: "55.0.28",
          "expo-file-system": "55.0.24",
          "expo-modules-core": "55.0.25",
          react: "19.2.0",
          "react-native": "0.83.6",
        },
      },
      null,
      2,
    )}\n`,
  );
  writeFileSync(
    resolve(applicationRoot, "app.json"),
    `${JSON.stringify(
      {
        expo: {
          name: "Midnight iOS Local Prover Validation",
          slug: "midnight-ios-local-prover-validation",
          version: "1.0.0",
          ios: { bundleIdentifier },
        },
      },
      null,
      2,
    )}\n`,
  );
  writeFileSync(
    resolve(applicationRoot, "index.js"),
    'import { registerRootComponent } from "expo";\nimport App from "./App";\nregisterRootComponent(App);\n',
  );
  writeFileSync(
    resolve(applicationRoot, "App.js"),
    applicationSource(manifest),
  );
  run("/bin/cp", [
    "-cR",
    resolve(repositoryRoot, "node_modules"),
    resolve(applicationRoot, "node_modules"),
  ]);
  const installedPackage = resolve(
    applicationRoot,
    "node_modules/@1am/midnight-mobile",
  );
  removeTree(installedPackage);
  cpSync(packageRoot, installedPackage, { recursive: true });
}

function prebuildConsumer() {
  run(
    process.execPath,
    [
      resolve(applicationRoot, "node_modules/expo/bin/cli"),
      "prebuild",
      "--platform",
      "ios",
      "--no-install",
      "--clean",
    ],
    { cwd: applicationRoot, inherit: true },
  );
  run(
    "pod",
    ["install", `--project-directory=${resolve(applicationRoot, "ios")}`],
    {
      cwd: applicationRoot,
      inherit: true,
    },
  );
}

function buildApplication() {
  const iosRoot = resolve(applicationRoot, "ios");
  const workspace = readdirSync(iosRoot)
    .filter((entry) => entry.endsWith(".xcworkspace"))
    .map((entry) => resolve(iosRoot, entry))[0];
  if (workspace === undefined)
    throw new Error("Expo prebuild produced no workspace");
  const listing = JSON.parse(
    run("xcodebuild", ["-workspace", workspace, "-list", "-json"]),
  );
  const schemes = listing.workspace?.schemes ?? [];
  const scheme = schemes.includes(listing.workspace?.name)
    ? listing.workspace.name
    : undefined;
  if (scheme === undefined) throw new Error("Expo workspace has no app scheme");
  const derivedData = resolve(targetRoot, "derived-data");
  if (!resumeApplicationBuild) removeTree(derivedData);
  run(
    "xcodebuild",
    [
      "-workspace",
      workspace,
      "-scheme",
      scheme,
      "-configuration",
      "Release",
      "-jobs",
      "2",
      "-quiet",
      "-destination",
      `platform=iOS Simulator,id=${device}`,
      "-derivedDataPath",
      derivedData,
      "CODE_SIGNING_ALLOWED=NO",
      "CODE_SIGNING_REQUIRED=NO",
      "ONLY_ACTIVE_ARCH=YES",
      "build",
    ],
    {
      cwd: applicationRoot,
      env: { ...process.env, NODE_BINARY: process.execPath },
      inherit: true,
    },
  );
  const app = findPath(
    resolve(derivedData, "Build/Products/Release-iphonesimulator"),
    (path, stat) => stat.isDirectory() && path.endsWith(".app"),
  );
  if (app === undefined)
    throw new Error("xcodebuild produced no simulator app");
  return { app, scheme };
}

function stageArtifactsAndSign(app) {
  cpSync(artifactsRoot, resolve(app, "LocalProverArtifacts"), {
    recursive: true,
  });
  const framework = resolve(app, "Frameworks/MidnightNativeRuntime.framework");
  if (!existsSync(framework))
    throw new Error("app did not embed runtime framework");
  run("codesign", ["--force", "--sign", "-", "--timestamp=none", framework]);
  run("codesign", ["--force", "--sign", "-", "--timestamp=none", app]);
  run("codesign", ["--verify", "--deep", "--strict", "--verbose=4", app]);
}

function installAndLaunch(app) {
  run("xcrun", ["simctl", "bootstatus", device, "-b"]);
  spawnSync("xcrun", ["simctl", "uninstall", device, bundleIdentifier], {
    cwd: repositoryRoot,
    encoding: "utf8",
  });
  run("xcrun", ["simctl", "install", device, app]);
  const launch = run("xcrun", [
    "simctl",
    "launch",
    "--terminate-running-process",
    device,
    bundleIdentifier,
  ]).trim();
  const dataContainer = run("xcrun", [
    "simctl",
    "get_app_container",
    device,
    bundleIdentifier,
    "data",
  ]).trim();
  return { dataContainer, launch };
}

function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
}

async function waitForResult(dataContainer) {
  const resultPath = resolve(
    dataContainer,
    "Documents/local-prover-result.json",
  );
  const started = Date.now();
  while (Date.now() - started < timeoutMillis) {
    if (existsSync(resultPath)) {
      try {
        return {
          elapsedMillis: Date.now() - started,
          result: JSON.parse(readFileSync(resultPath, "utf8")),
        };
      } catch {
        // The application writes this small completion marker last; retry a partial read.
      }
    }
    await delay(500);
  }
  throw new Error("simulator local prover timed out");
}

function hostValidate(dataContainer) {
  const documents = resolve(dataContainer, "Documents");
  const check = resolve(documents, "check-response.bin");
  const proof = resolve(documents, "proof-v2.bin");
  for (const [path, example] of [
    [check, "validate_android_prover_check"],
    [proof, "validate_android_prover_proof"],
  ]) {
    if (!existsSync(path))
      throw new Error(`application result is missing ${path}`);
    run(
      "cargo",
      [
        "run",
        "--offline",
        "--locked",
        "--quiet",
        "--package",
        "midnight-native-runtime",
        "--features",
        "local-prover",
        "--example",
        example,
      ],
      { input: readFileSync(path) },
    );
  }
  return {
    check: { size: statSync(check).size, sha256: sha256(check) },
    proof: { size: statSync(proof).size, sha256: sha256(proof) },
  };
}

async function main() {
  const manifest = prepareArtifacts();
  const simulator = simulatorDetails();
  if (!resumeApplicationBuild) {
    run("npm", ["run", "build", "--workspace", "@1am/midnight-mobile"], {
      inherit: true,
    });
    run(
      "node",
      [
        resolve(repositoryRoot, "scripts/build-apple-xcframework.mjs"),
        "--single-pass",
      ],
      { inherit: true },
    );
    createConsumer(manifest);
    prebuildConsumer();
  }
  const build = buildApplication();
  stageArtifactsAndSign(build.app);
  const launched = installAndLaunch(build.app);
  const completion = await waitForResult(launched.dataContainer);
  if (completion.result.success !== true) {
    throw new Error(
      `simulator prover failed: ${JSON.stringify(completion.result)}`,
    );
  }
  const outputs = hostValidate(launched.dataContainer);
  const runtimeFramework = resolve(
    build.app,
    "Frameworks/MidnightNativeRuntime.framework",
  );
  const provingArtifactBytes = manifest.artifacts.reduce(
    (total, artifact) => total + artifact.size,
    0,
  );
  const report = {
    schemaVersion: 1,
    capturedAt: new Date().toISOString(),
    simulator,
    bundleIdentifier,
    scheme: build.scheme,
    launch: launched.launch,
    elapsedMillis: completion.elapsedMillis,
    nativeResult: completion.result,
    outputs,
    sizes: {
      nativeFrameworkBytes: treeBytes(runtimeFramework),
      nativeBinaryBytes: statSync(
        resolve(runtimeFramework, "MidnightNativeRuntime"),
      ).size,
      xcframeworkBytes: treeBytes(xcframework),
      xcframeworkArchiveBytes: statSync(xcframeworkArchive).size,
      provingArtifactBytes,
      stagedArtifactDirectoryBytes: treeBytes(
        resolve(build.app, "LocalProverArtifacts"),
      ),
      applicationBytes: treeBytes(build.app),
    },
    xcframework,
    xcframeworkArchive,
    xcframeworkArchiveSha256: sha256(xcframeworkArchive),
  };
  writeFileSync(
    resolve(targetRoot, "simulator-run-report.json"),
    `${JSON.stringify(report, null, 2)}\n`,
  );
  console.log(
    `iOS local prover passed: check=${String(outputs.check.size)} ` +
      `proof=${String(outputs.proof.size)} elapsedMs=${String(report.elapsedMillis)}`,
  );
}

await main();
