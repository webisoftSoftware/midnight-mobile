import { createHash } from "node:crypto";
import { mkdtemp, readdir, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, relative } from "node:path";
import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";

const require = createRequire(import.meta.url);
const expoCli = require.resolve("expo/bin/cli");

async function filesUnder(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = await Promise.all(
    entries.map(async (entry) => {
      const path = join(directory, entry.name);
      return entry.isDirectory() ? filesUnder(path) : [path];
    }),
  );
  return files.flat();
}

function exportPlatform(platform, outputDirectory) {
  const result = spawnSync(
    process.execPath,
    [
      expoCli,
      "export",
      "--platform",
      platform,
      "--output-dir",
      outputDirectory,
      "--clear",
      "--no-bytecode",
    ],
    {
      cwd: new URL("..", import.meta.url),
      encoding: "utf8",
      env: {
        ...process.env,
        CI: "1",
        EXPO_NO_TELEMETRY: "1",
      },
    },
  );
  if (result.status !== 0) {
    throw new Error(
      result.stderr || result.stdout || `${platform} export failed`,
    );
  }
}

async function bundleFingerprint(directory, platform) {
  const files = await filesUnder(directory);
  const marker = `/_expo/static/js/${platform}/`;
  const bundles = files.filter((file) =>
    file.replaceAll("\\", "/").includes(marker),
  );
  if (bundles.length === 0) {
    throw new Error(`Expo produced no ${platform} JavaScript bundle`);
  }
  const digest = createHash("sha256");
  for (const file of bundles.sort()) {
    digest.update(relative(directory, file));
    digest.update(await readFile(file));
  }
  return digest.digest("hex");
}

async function validatePlatform(root, platform) {
  const first = join(root, `${platform}-first`);
  const second = join(root, `${platform}-second`);
  exportPlatform(platform, first);
  exportPlatform(platform, second);
  const firstFingerprint = await bundleFingerprint(first, platform);
  const secondFingerprint = await bundleFingerprint(second, platform);
  if (firstFingerprint !== secondFingerprint) {
    throw new Error(`${platform} JavaScript export was not reproducible`);
  }
  console.log(`${platform} deterministic bundle ${firstFingerprint}`);
}

const temporaryRoot = await mkdtemp(join(tmpdir(), "midnight-expo-m3-"));
try {
  await validatePlatform(temporaryRoot, "ios");
  await validatePlatform(temporaryRoot, "android");
} finally {
  await rm(temporaryRoot, { recursive: true, force: true });
}
