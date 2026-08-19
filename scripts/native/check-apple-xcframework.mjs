import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  realpathSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import {
  createCompressedArchive,
  REPOSITORY_ROOT,
  run,
  validateXcframework,
  XCFRAMEWORK_BUNDLE,
} from "./apple-native.mjs";
import { validateAppleConsumer } from "./apple-consumer.mjs";
import { validateTarballExpoConsumer } from "./apple-expo-consumer.mjs";
import { removeTree } from "../quality/quality-utils.mjs";

const defaultXcframework = join(
  REPOSITORY_ROOT,
  "packages/react-native/ios/build",
  XCFRAMEWORK_BUNDLE,
);
const xcframework = resolve(process.argv[2] ?? defaultXcframework);
const temporaryRoot = realpathSync(
  mkdtempSync(join(tmpdir(), "midnight-apple-check-")),
);

function installPackedPackage(root) {
  const packageRoot = join(REPOSITORY_ROOT, "packages/react-native");
  const packedRoot = join(root, "packed");
  mkdirSync(packedRoot, { recursive: true });
  const report = JSON.parse(
    run(
      "npm",
      ["pack", "--json", "--pack-destination", packedRoot, packageRoot],
      {
        cwd: REPOSITORY_ROOT,
        env: {
          ...process.env,
          npm_config_cache: join(root, "npm-cache"),
        },
      },
    ),
  );
  const filename = report?.[0]?.filename;
  if (typeof filename !== "string") {
    throw new Error("Apple native gate: npm pack returned no tarball");
  }
  const installation = join(root, "installation");
  mkdirSync(installation, { recursive: true });
  writeFileSync(
    join(installation, "package.json"),
    '{"name":"midnight-apple-consumer","private":true}\n',
  );
  run(
    "npm",
    [
      "install",
      "--offline",
      "--ignore-scripts",
      "--legacy-peer-deps",
      "--no-audit",
      "--no-fund",
      join(packedRoot, filename),
    ],
    {
      cwd: installation,
      env: {
        ...process.env,
        npm_config_cache: join(root, "npm-cache"),
      },
    },
  );
  const installed = join(installation, "node_modules/@1am/midnight-mobile");
  if (!existsSync(installed)) {
    throw new Error("Apple native gate: packed package was not installed");
  }
  return { installed, tarball: join(packedRoot, filename) };
}

try {
  const fingerprint = validateXcframework(xcframework);
  const firstArchive = join(temporaryRoot, `first-${XCFRAMEWORK_BUNDLE}.zip`);
  const secondArchive = join(temporaryRoot, `second-${XCFRAMEWORK_BUNDLE}.zip`);
  const archiveSize = createCompressedArchive(
    xcframework,
    firstArchive,
    join(temporaryRoot, "first-staging"),
  );
  createCompressedArchive(
    xcframework,
    secondArchive,
    join(temporaryRoot, "second-staging"),
  );
  run("/usr/bin/cmp", ["-s", firstArchive, secondArchive]);
  const packedPackage = installPackedPackage(temporaryRoot);
  const installedPackage = packedPackage.installed;
  const installedXcframework = join(
    installedPackage,
    "ios/build",
    XCFRAMEWORK_BUNDLE,
  );
  const installedFingerprint = validateXcframework(installedXcframework);
  if (installedFingerprint.sha256 !== fingerprint.sha256) {
    throw new Error("Apple native gate: packed XCFramework bytes changed");
  }
  const consumer = validateAppleConsumer(
    installedXcframework,
    join(installedPackage, "ios/MidnightMobileRuntime.podspec"),
    join(temporaryRoot, "consumer"),
  );
  const expoConsumer = validateTarballExpoConsumer(
    packedPackage.installed,
    join(temporaryRoot, "expo-consumer"),
  );
  console.log(
    `Apple XCFramework valid: files=${String(
      fingerprint.files.length,
    )}, sha256=${fingerprint.sha256}, archive-bytes=${String(archiveSize)}`,
  );
  console.log(
    `Apple clean consumer valid: swift-import=${consumer.swiftImports}, rust-path=${consumer.rustPath}, signing=${consumer.signing}`,
  );
  console.log(
    `Apple Expo tarball consumer valid: scheme=${expoConsumer.scheme}, signing=${expoConsumer.signing}`,
  );
} finally {
  removeTree(temporaryRoot);
}
