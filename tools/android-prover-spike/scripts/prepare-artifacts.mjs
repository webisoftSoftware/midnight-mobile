import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  createReadStream,
  existsSync,
  mkdirSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(scriptDirectory, "../../..");
const outputRoot = resolve(
  repositoryRoot,
  "target/android-prover-spike/artifacts",
);
const source = "https://srs.midnight.network";

const artifacts = Object.freeze([
  {
    path: "bls_midnight_2p15",
    sha256: "724c7c3d779148bb113c7ee9c034b2f27db16e6bdf315fde90105a9bad00b1de",
    size: 6_291_844,
  },
  {
    path: "zswap/9/spend.prover",
    sha256: "19d234b5c68b7212ad6b0ec9334a95594748154128f3704eb576bcc843cc5c45",
    size: 11_020_001,
  },
  {
    path: "zswap/9/spend.verifier",
    sha256: "544554effd7ae9fb9063be52a9ec2a986756301071fcd97bb4598fb45a335658",
    size: 2_311,
  },
  {
    path: "zswap/9/spend.bzkir",
    sha256: "7cb5bbcf67cb212a3336fb439a77e8f32f0aa8a56185c8e1247d6cbfc7300205",
    size: 1_294,
  },
]);

function run(command, arguments_, options = {}) {
  const result = spawnSync(command, arguments_, {
    cwd: repositoryRoot,
    stdio: "inherit",
    ...options,
  });
  if (result.error !== undefined || result.status !== 0) {
    throw new Error(`${command} failed with status ${String(result.status)}`);
  }
  return result;
}

function sha256(path) {
  return new Promise((resolveHash, reject) => {
    const hash = createHash("sha256");
    const stream = createReadStream(path);
    stream.on("error", reject);
    stream.on("data", (chunk) => hash.update(chunk));
    stream.on("end", () => resolveHash(hash.digest("hex")));
  });
}

async function verify(path, artifact) {
  if (statSync(path).size !== artifact.size) return false;
  return (await sha256(path)) === artifact.sha256;
}

async function prepareArtifact(artifact) {
  const destination = resolve(outputRoot, artifact.path);
  mkdirSync(dirname(destination), { recursive: true });
  if (existsSync(destination) && (await verify(destination, artifact))) return;
  const temporary = `${destination}.download`;
  rmSync(temporary, { force: true });
  run("curl", [
    "--fail",
    "--location",
    "--retry",
    "3",
    "--output",
    temporary,
    `${source}/${artifact.path}`,
  ]);
  if (!(await verify(temporary, artifact))) {
    rmSync(temporary, { force: true });
    throw new Error(`integrity check failed for ${artifact.path}`);
  }
  renameSync(temporary, destination);
}

async function main() {
  mkdirSync(outputRoot, { recursive: true });
  for (const artifact of artifacts) await prepareArtifact(artifact);
  const request = run(
    "cargo",
    [
      "run",
      "--offline",
      "--locked",
      "--quiet",
      "--package",
      "midnight-native-runtime",
      "--features",
      "android-prover-spike",
      "--example",
      "generate_android_prover_request",
    ],
    {
      encoding: null,
      maxBuffer: 64 * 1024 * 1024,
      stdio: ["ignore", "pipe", "inherit"],
    },
  ).stdout;
  writeFileSync(resolve(outputRoot, "request.bin"), request);
  const manifest = {
    schemaVersion: 1,
    source,
    ledgerRevision: "02716c2c95d50654aeb3cb63bfd8386046e4ca7d",
    artifacts,
    request: {
      path: "request.bin",
      sha256: createHash("sha256").update(request).digest("hex"),
      size: request.length,
    },
  };
  writeFileSync(
    resolve(outputRoot, "artifact-manifest.json"),
    `${JSON.stringify(manifest, null, 2)}\n`,
  );
  console.log(
    `staged ${String(artifacts.length)} verified artifacts in ${outputRoot}`,
  );
}

await main();
