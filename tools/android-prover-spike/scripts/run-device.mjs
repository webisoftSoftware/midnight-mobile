import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = resolve(
  dirname(fileURLToPath(import.meta.url)),
  "../../..",
);
const targetRoot = resolve(repositoryRoot, "target/android-prover-spike");
const apk = resolve(targetRoot, "android-prover-spike-release.apk");
const packageName = "network.midnight.proverspike";
const activity = `${packageName}/.MainActivity`;
const timeoutMillis = 15 * 60 * 1_000;

function run(command, arguments_, options = {}) {
  const result = spawnSync(command, arguments_, {
    cwd: repositoryRoot,
    encoding: "utf8",
    maxBuffer: 30 * 1024 * 1024,
    ...options,
  });
  if (result.error !== undefined || result.status !== 0) {
    throw new Error(
      `${command} ${arguments_.join(" ")} failed: ${result.stderr ?? ""}`,
    );
  }
  return result.stdout.trim();
}

function adb(...arguments_) {
  return run("adb", arguments_);
}

function shell(command) {
  return adb("shell", command);
}

function tryShell(command) {
  const result = spawnSync("adb", ["shell", command], {
    cwd: repositoryRoot,
    encoding: "utf8",
    maxBuffer: 30 * 1024 * 1024,
  });
  return result.status === 0 ? result.stdout.trim() : "";
}

function deviceSnapshot() {
  return {
    capturedAt: new Date().toISOString(),
    manufacturer: shell("getprop ro.product.manufacturer"),
    model: shell("getprop ro.product.model"),
    androidVersion: shell("getprop ro.build.version.release"),
    apiLevel: Number(shell("getprop ro.build.version.sdk")),
    abi: shell("getprop ro.product.cpu.abi"),
    cpuCount: Number(shell("nproc")),
    memTotalKb: Number(
      /MemTotal:\s+(\d+)/u.exec(shell("cat /proc/meminfo"))?.[1] ?? 0,
    ),
    battery: shell("dumpsys battery"),
    thermal: shell("dumpsys thermalservice"),
  };
}

function memorySample(pid, elapsedMillis) {
  const meminfo = tryShell(`dumpsys meminfo ${pid}`);
  const summary = /TOTAL PSS:\s*(\d+).*?TOTAL RSS:\s*(\d+)/su.exec(meminfo);
  const fallback = /^\s*TOTAL\s+(\d+)/mu.exec(meminfo);
  const status = tryShell(`run-as ${packageName} cat /proc/${pid}/status`);
  return {
    elapsedMillis,
    pssKb: Number(summary?.[1] ?? fallback?.[1] ?? 0),
    rssKb: Number(summary?.[2] ?? 0),
    vmHwmKb: Number(/^VmHWM:\s+(\d+)/mu.exec(status)?.[1] ?? 0),
    threads: Number(/^Threads:\s+(\d+)/mu.exec(status)?.[1] ?? 0),
  };
}

function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
}

function probeResult() {
  const logs = adb("logcat", "-d", "-s", "EmbeddedProverSpike:I", "*:S");
  return logs.split("\n").findLast((line) => line.includes("PROBE_RESULT"));
}

function pullAndValidate(name, destinationName, example) {
  const result = spawnSync(
    "adb",
    ["exec-out", "run-as", packageName, "cat", `files/${name}`],
    { cwd: repositoryRoot, encoding: null, maxBuffer: 64 * 1024 * 1024 },
  );
  if (result.status !== 0)
    throw new Error("unable to pull proof from application storage");
  const outputPath = resolve(targetRoot, destinationName);
  writeFileSync(outputPath, result.stdout);
  run(
    "cargo",
    [
      "run",
      "--offline",
      "--locked",
      "--quiet",
      "--package",
      "midnight-mobile-runtime",
      "--features",
      "local-prover",
      "--example",
      example,
    ],
    { input: result.stdout },
  );
  return outputPath;
}

async function monitorProbe(started) {
  const startupDeadline = started + 10_000;
  const samples = [];
  let resultLine;
  while (Date.now() - started < timeoutMillis) {
    const pid = tryShell(`pidof ${packageName}`);
    if (pid.length === 0) {
      resultLine = probeResult();
      if (resultLine !== undefined || Date.now() >= startupDeadline) break;
      await delay(250);
      continue;
    }
    samples.push(memorySample(pid, Date.now() - started));
    resultLine = probeResult();
    if (resultLine !== undefined) break;
    await delay(2_000);
  }
  return { resultLine, samples };
}

function captureAfterSnapshot() {
  try {
    return deviceSnapshot();
  } catch (error) {
    return { capturedAt: new Date().toISOString(), error: String(error) };
  }
}

function validateSuccessfulProof(resultLine, alive) {
  if (resultLine?.includes("status=SUCCESS") !== true || !alive) {
    return { success: false };
  }
  try {
    const checkPath = pullAndValidate(
      "check-response.bin",
      "device-check-response.bin",
      "validate_android_prover_check",
    );
    const proofPath = pullAndValidate(
      "proof-v2.bin",
      "device-proof-v2.bin",
      "validate_android_prover_proof",
    );
    return { success: true, proofPath, checkPath };
  } catch (error) {
    return { success: false, hostValidationError: String(error) };
  }
}

async function main() {
  if (!existsSync(apk)) throw new Error(`APK is missing: ${apk}`);
  adb("wait-for-device");
  const before = deviceSnapshot();
  adb("install", "-r", apk);
  shell(`am force-stop ${packageName}`);
  adb("logcat", "-c");
  shell(`am start -n ${activity} --ez runProbe true`);
  const started = Date.now();
  const { resultLine, samples } = await monitorProbe(started);
  const elapsedMillis = Date.now() - started;
  const alive = tryShell(`pidof ${packageName}`).length > 0;
  const timedOut = resultLine === undefined && elapsedMillis >= timeoutMillis;
  const after = captureAfterSnapshot();
  const { success, proofPath, checkPath, hostValidationError } =
    validateSuccessfulProof(resultLine, alive);
  const report = {
    schemaVersion: 1,
    timeoutMillis,
    before,
    after,
    elapsedMillis,
    timedOut,
    processAlive: alive,
    resultLine: resultLine ?? null,
    proofPath: proofPath ?? null,
    checkPath: checkPath ?? null,
    hostValidationError: hostValidationError ?? null,
    samples,
    peaks: {
      pssKb: Math.max(0, ...samples.map((sample) => sample.pssKb)),
      rssKb: Math.max(0, ...samples.map((sample) => sample.rssKb)),
      vmHwmKb: Math.max(0, ...samples.map((sample) => sample.vmHwmKb)),
      threads: Math.max(0, ...samples.map((sample) => sample.threads)),
    },
    success,
  };
  mkdirSync(targetRoot, { recursive: true });
  writeFileSync(
    resolve(targetRoot, "device-run-report.json"),
    `${JSON.stringify(report, null, 2)}\n`,
  );
  console.log(
    `device proof success=${String(success)} elapsedMs=${String(elapsedMillis)}`,
  );
  if (timedOut) tryShell(`am force-stop ${packageName}`);
  if (!success) process.exitCode = 1;
}

await main();
