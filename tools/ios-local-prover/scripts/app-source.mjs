// The source of the validation application the simulator harness builds.
//
// The application proves the staged Zswap spend request on the CPU, then, when
// the native prover has an accelerator, turns it on and proves the same request
// again. It writes its proofs and a JSON report to its Documents directory,
// where the harness reads them.

const TEMPLATE = `import { createMidnightLocalProver } from "@1am/midnight-mobile/local-prover";
import { File, Paths } from "expo-file-system";
import { useEffect, useState } from "react";
import { Text, View } from "react-native";

const configuration = __CONFIGURATION__;
const checkRequest = new Uint8Array(__CHECK_REQUEST__);
const proveRequest = new Uint8Array(__PROVE_REQUEST__);

function write(name, content) {
  const file = new File(Paths.document, name);
  file.create({ overwrite: true });
  file.write(content);
}

// Proves the request twice and returns the last proof with both durations.
// The first proof of a mode is cold: with the GPU on it also builds the GPU's
// copy of the proof parameters.
async function proveTwice(prover) {
  const startedAt = performance.now();
  await prover.prove(proveRequest);
  const coldAt = performance.now();
  const proof = await prover.prove(proveRequest);
  return {
    proof,
    coldMillis: coldAt - startedAt,
    warmMillis: performance.now() - coldAt,
  };
}

// Turns the accelerator on, proves, and reports what the GPU did.
async function accelerated(prover) {
  if (
    typeof prover.setAccelerator !== "function" ||
    typeof prover.getAcceleratorStatus !== "function"
  ) {
    return { supported: false };
  }
  await prover.setAccelerator(true);
  try {
    const before = await prover.getAcceleratorStatus();
    if (!before.active) {
      return { supported: true, active: false, availability: before.availability };
    }
    const run = await proveTwice(prover);
    const after = await prover.getAcceleratorStatus();
    write("proof-v2-accelerated.bin", run.proof);
    return {
      supported: true,
      active: after.active,
      availability: after.availability,
      device: after.device,
      tripped: after.tripped,
      cpuRetries: after.cpuRetries,
      gpuOperations: after.gpuOperations - before.gpuOperations,
      cpuOperations: after.cpuOperations - before.cpuOperations,
      deviceMicros: after.deviceMicros - before.deviceMicros,
      coldProveMillis: run.coldMillis,
      warmProveMillis: run.warmMillis,
      proofBytes: run.proof.length,
    };
  } finally {
    await prover.setAccelerator(false);
  }
}

// The report as text, so that a run can be read off the screen of a device
// whose files the host cannot reach.
function summary(report) {
  const ms = (value) =>
    typeof value === "number" ? String(Math.round(value)) + " ms" : "n/a";
  const gpu = report.accelerator ?? {};
  const lines = [
    "Local check and proof succeeded",
    "CPU: first " + ms(report.proveMillis) + ", repeat " + ms(report.warmProveMillis),
  ];
  if (gpu.active === true) {
    lines.push(
      "GPU: first " + ms(gpu.coldProveMillis) + ", repeat " + ms(gpu.warmProveMillis),
      String(gpu.gpuOperations) + " GPU operations on " + String(gpu.device),
    );
  } else {
    lines.push("GPU not used: " + String(gpu.availability ?? "not available"));
  }
  return lines.join("\\n");
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
        const cpu = await proveTwice(prover);
        write("check-response.bin", checked);
        write("proof-v2.bin", cpu.proof);
        report = {
          success: true,
          configureMillis: configuredAt - startedAt,
          checkMillis: checkedAt - configuredAt,
          proveMillis: cpu.coldMillis,
          warmProveMillis: cpu.warmMillis,
          checkBytes: checked.length,
          proofBytes: cpu.proof.length,
          accelerator: await accelerated(prover),
        };
        setStatus(summary(report));
      } catch (error) {
        report = {
          success: false,
          code: typeof error === "object" && error !== null && "code" in error
            ? String(error.code)
            : "UNEXPECTED_ERROR",
        };
        setStatus("Local prover failed: " + report.code);
      } finally {
        if (prover !== undefined) {
          try { await prover.close(); } catch { report.closeFailed = true; }
        }
        write("local-prover-result.json", JSON.stringify(report));
      }
    })();
  }, []);
  return (
    <View style={{ flex: 1, justifyContent: "center", padding: 24 }}>
      <Text testID="local-prover-status" style={{ fontSize: 18 }}>{status}</Text>
    </View>
  );
}
`;

/** The prover configuration of the application: the staged Zswap spend circuit. */
export function localProverConfiguration(manifest) {
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

export function applicationSource({
  configuration,
  checkRequest,
  proveRequest,
}) {
  return TEMPLATE.replace("__CONFIGURATION__", () =>
    JSON.stringify(configuration),
  )
    .replace("__CHECK_REQUEST__", () => JSON.stringify([...checkRequest]))
    .replace("__PROVE_REQUEST__", () => JSON.stringify([...proveRequest]));
}
