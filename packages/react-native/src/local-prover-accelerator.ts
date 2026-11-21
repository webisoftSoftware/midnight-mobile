/**
 * Why GPU acceleration of proving is or is not in use.
 *
 * - `not-requested`: the host has not turned the accelerator on.
 * - `active`: the device has a usable GPU and proofs use it.
 * - `unsupported`: this build has no accelerator for the platform.
 * - `no-device`: no usable GPU, or an operating system older than the
 *   accelerator needs. Proofs run on the CPU.
 * - `hooks-taken`: another component claimed the prover's accelerator slots.
 */
export type MidnightLocalProverAcceleratorAvailability =
  "not-requested" | "active" | "unsupported" | "no-device" | "hooks-taken";

export interface MidnightLocalProverAcceleratorStatus {
  /** Whether the host asked for the accelerator. */
  readonly requested: boolean;
  readonly availability: MidnightLocalProverAcceleratorAvailability;
  /** Whether proofs use the GPU right now. */
  readonly active: boolean;
  /**
   * Whether a GPU fault turned the accelerator off. A proof that fails after
   * the GPU worked on it is made again on the CPU; when that succeeds the
   * accelerator stays off until `setAccelerator(true)` is called again.
   */
  readonly tripped: boolean;
  /** Proofs that failed with the GPU and were made on the CPU instead. */
  readonly cpuRetries: number;
  /** The GPU's name, when one is in use. */
  readonly device: string | null;
  /** Prover operations the GPU computed since the process started. */
  readonly gpuOperations: number;
  /** Operations offered to the GPU that ran on the CPU instead. */
  readonly cpuOperations: number;
  /** GPU time of the served operations, in microseconds. */
  readonly deviceMicros: number;
  /** Proving-parameter tables held for the GPU, and the points in them. */
  readonly tables: number;
  readonly tablePoints: number;
}

const AVAILABILITIES: ReadonlySet<string> = new Set([
  "not-requested",
  "active",
  "unsupported",
  "no-device",
  "hooks-taken",
]);

const COUNTERS = [
  "cpuRetries",
  "gpuOperations",
  "cpuOperations",
  "deviceMicros",
  "tables",
  "tablePoints",
] as const;

function isCount(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

/**
 * Decodes the status object the native prover reports. Returns `undefined`
 * for anything that is not exactly that object, so a caller never acts on a
 * half-understood status.
 */
export function decodeAcceleratorStatus(
  json: string,
): MidnightLocalProverAcceleratorStatus | undefined {
  let parsed: unknown;
  try {
    parsed = JSON.parse(json);
  } catch {
    return undefined;
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    return undefined;
  }
  const value = parsed as Readonly<Record<string, unknown>>;
  const { requested, availability, active, tripped, device } = value;
  if (
    typeof requested !== "boolean" ||
    typeof active !== "boolean" ||
    typeof tripped !== "boolean" ||
    typeof availability !== "string" ||
    !AVAILABILITIES.has(availability) ||
    (device !== null && typeof device !== "string") ||
    !COUNTERS.every((name) => isCount(value[name]))
  ) {
    return undefined;
  }
  const count = (name: (typeof COUNTERS)[number]): number =>
    value[name] as number;
  return {
    requested,
    availability: availability as MidnightLocalProverAcceleratorAvailability,
    active,
    tripped,
    cpuRetries: count("cpuRetries"),
    device,
    gpuOperations: count("gpuOperations"),
    cpuOperations: count("cpuOperations"),
    deviceMicros: count("deviceMicros"),
    tables: count("tables"),
    tablePoints: count("tablePoints"),
  };
}
