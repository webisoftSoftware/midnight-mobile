export type MidnightRuntimeErrorCode =
  | "INVALID_ARGUMENT"
  | "STALE_SESSION"
  | "CANCELLED"
  | "STATE_INCOMPATIBLE"
  | "SYNC_GAP"
  | "PROOF_FAILED"
  | "INSUFFICIENT_DUST"
  | "SUBMISSION_STATUS_UNKNOWN"
  | "TRANSPORT_ERROR"
  | "NATIVE_INTERNAL"
  | "UNAVAILABLE";

export class MidnightRuntimeError extends Error {
  readonly code: MidnightRuntimeErrorCode;

  constructor(code: MidnightRuntimeErrorCode) {
    super(code);
    this.name = "MidnightRuntimeError";
    this.code = code;
  }
}

const CODES = new Set<MidnightRuntimeErrorCode>([
  "INVALID_ARGUMENT",
  "STALE_SESSION",
  "CANCELLED",
  "STATE_INCOMPATIBLE",
  "SYNC_GAP",
  "PROOF_FAILED",
  "INSUFFICIENT_DUST",
  "SUBMISSION_STATUS_UNKNOWN",
  "TRANSPORT_ERROR",
  "NATIVE_INTERNAL",
  "UNAVAILABLE",
]);

function candidateCode(value: unknown): MidnightRuntimeErrorCode | null {
  if (typeof value !== "string") return null;
  const canonical = value
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
    .replace(/[^A-Za-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .toUpperCase();
  return CODES.has(canonical as MidnightRuntimeErrorCode)
    ? (canonical as MidnightRuntimeErrorCode)
    : null;
}

export function normalizeMidnightError(error: unknown): MidnightRuntimeError {
  if (error instanceof MidnightRuntimeError) return error;
  if (typeof error === "object" && error !== null) {
    const record = error as {
      code?: unknown;
      message?: unknown;
      name?: unknown;
    };
    for (const value of [record.code, record.name, record.message]) {
      const code = candidateCode(value);
      if (code) return new MidnightRuntimeError(code);
    }
  }
  const code = candidateCode(error);
  return new MidnightRuntimeError(code ?? "NATIVE_INTERNAL");
}
