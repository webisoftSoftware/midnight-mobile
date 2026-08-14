import { requireNativeModule } from "expo-modules-core";

import {
  createMidnightTransportWithProofAdapter,
  type MidnightStandardTransport,
  type MidnightTransportConfiguration,
} from "./transport.js";

export const EXPO_MIDNIGHT_LOCAL_PROVER_MODULE_NAME =
  "MidnightMobileLocalProver";

export type MidnightLocalProverErrorCode =
  | "INVALID_REQUEST"
  | "UNSUPPORTED_CIRCUIT"
  | "INTEGRITY_CHECK_FAILED"
  | "PROVER_BUSY"
  | "RESOURCE_PREFLIGHT_FAILED"
  | "PROOF_FAILED"
  | "INVALID_CONFIGURATION"
  | "STALE_REGISTRY"
  | "CHECK_FAILED"
  | "NATIVE_INTERNAL"
  | "CIRCUIT_TOO_LARGE";

const ERROR_CODES = new Set<MidnightLocalProverErrorCode>([
  "INVALID_REQUEST",
  "UNSUPPORTED_CIRCUIT",
  "INTEGRITY_CHECK_FAILED",
  "PROVER_BUSY",
  "RESOURCE_PREFLIGHT_FAILED",
  "PROOF_FAILED",
  "INVALID_CONFIGURATION",
  "STALE_REGISTRY",
  "CHECK_FAILED",
  "NATIVE_INTERNAL",
  "CIRCUIT_TOO_LARGE",
]);

/**
 * How the native bridges report the circuit size behind a `CIRCUIT_TOO_LARGE`.
 *
 * The C ABI returns a single `i32`, so the size rides in the status' high bits and
 * the Kotlin/Swift bridges append it to the error code string — `Exception` has no
 * other field that survives the trip into JS. Parsed back into a number here so
 * callers never have to read a message.
 */
const CIRCUIT_TOO_LARGE_PATTERN = /^CIRCUIT_TOO_LARGE k=(\d{1,3})$/u;

export class MidnightLocalProverError extends Error {
  readonly code: MidnightLocalProverErrorCode;
  /** Circuit size the refused job needed. Only set for `CIRCUIT_TOO_LARGE`. */
  readonly requiredK?: number;

  constructor(
    code: MidnightLocalProverErrorCode,
    cause?: unknown,
    requiredK?: number,
  ) {
    super(code, { cause });
    this.name = "MidnightLocalProverError";
    this.code = code;
    if (requiredK !== undefined) this.requiredK = requiredK;
  }
}

export interface MidnightLocalProverFile {
  /** `asset://` on Android, `bundle://` on iOS, or an absolute sandbox path. */
  readonly uri: string;
  readonly size: number;
  readonly sha256: string;
}

export interface MidnightLocalProverParameter {
  readonly k: number;
  readonly file: MidnightLocalProverFile;
}

export interface MidnightLocalProverCircuit {
  readonly keyLocation: string;
  readonly proverKey: MidnightLocalProverFile;
  readonly verifierKey: MidnightLocalProverFile;
  readonly ir: MidnightLocalProverFile;
}

export interface MidnightLocalProverConfiguration {
  readonly parameters: readonly MidnightLocalProverParameter[];
  readonly circuits: readonly MidnightLocalProverCircuit[];
}

export interface MidnightLocalProver {
  configure(configuration: MidnightLocalProverConfiguration): Promise<void>;
  check(request: Uint8Array): Promise<Uint8Array>;
  prove(request: Uint8Array): Promise<Uint8Array>;
  /**
   * Optional native fast path: proves every request in one native call.
   * Only present when the underlying native module implements it (older
   * native binaries do not); callers must be prepared to fall back to
   * individual `prove` calls when this is `undefined`.
   */
  proveBatch?(requests: readonly Uint8Array[]): Promise<Uint8Array[]>;
  /**
   * Optional: turns the native per-proof stage instrumentation on or off.
   *
   * Absent on native binaries built before the instrumentation was exposed.
   * Enabling it makes every proof additionally re-run the IR load and key init
   * it would otherwise do once, to price what an initialized-key cache would
   * save — so it measures at the cost of what it measures, and belongs in a
   * measurement run rather than in normal operation.
   */
  setProfiling?(enabled: boolean): Promise<void>;
  /**
   * Optional: drains recorded stage samples as a JSON array string, oldest
   * first, and empties the native queue. Yields `[]` when nothing was recorded,
   * which is what a caller should expect whenever profiling is off.
   */
  takeTimings?(): Promise<string>;
  close(): Promise<void>;
}

export interface NativeLocalProverModule {
  configure(configurationJson: string): Promise<number>;
  check(request: Uint8Array): Promise<Uint8Array>;
  prove(request: Uint8Array): Promise<Uint8Array>;
  /** Optional: may be absent on older native binaries. */
  proveBatch?(requests: readonly Uint8Array[]): Promise<Uint8Array[]>;
  /** Optional: may be absent on older native binaries. */
  setProfiling?(enabled: boolean): Promise<void>;
  /** Optional: may be absent on older native binaries. */
  takeTimings?(): Promise<string>;
  close(): Promise<void>;
}

export type NativeLocalProverModuleLoader =
  () => Partial<NativeLocalProverModule>;

export interface CreateMidnightLocalProverOptions {
  readonly nativeModuleLoader?: NativeLocalProverModuleLoader;
}

function defaultLoader(): NativeLocalProverModule {
  return requireNativeModule<NativeLocalProverModule>(
    EXPO_MIDNIGHT_LOCAL_PROVER_MODULE_NAME,
  );
}

function nativeModule(
  loader: NativeLocalProverModuleLoader | undefined,
): NativeLocalProverModule {
  const candidate = loader?.() ?? defaultLoader();
  for (const method of ["configure", "check", "prove", "close"] as const) {
    if (typeof candidate[method] !== "function") {
      throw new MidnightLocalProverError("NATIVE_INTERNAL");
    }
  }
  return candidate as NativeLocalProverModule;
}

interface DecodedNativeError {
  code: MidnightLocalProverErrorCode;
  requiredK?: number;
}

function decodeNativeError(error: unknown): DecodedNativeError {
  if (error instanceof MidnightLocalProverError) {
    return error.requiredK === undefined
      ? { code: error.code }
      : { code: error.code, requiredK: error.requiredK };
  }
  if (typeof error === "object" && error !== null && "code" in error) {
    const code = Reflect.get(error, "code");
    if (typeof code === "string") {
      if (ERROR_CODES.has(code as MidnightLocalProverErrorCode)) {
        return { code: code as MidnightLocalProverErrorCode };
      }
      const sized = CIRCUIT_TOO_LARGE_PATTERN.exec(code);
      if (sized) {
        return { code: "CIRCUIT_TOO_LARGE", requiredK: Number(sized[1]) };
      }
    }
  }
  return { code: "NATIVE_INTERNAL" };
}

async function nativeCall<T>(operation: () => Promise<T>): Promise<T> {
  try {
    return await operation();
  } catch (error) {
    const { code, requiredK } = decodeNativeError(error);
    throw new MidnightLocalProverError(code, error, requiredK);
  }
}

function validUri(uri: string): boolean {
  return (
    uri.startsWith("asset://") ||
    uri.startsWith("bundle://") ||
    uri.startsWith("/")
  );
}

function validateFile(file: MidnightLocalProverFile): void {
  if (
    !validUri(file.uri) ||
    !Number.isSafeInteger(file.size) ||
    file.size <= 0 ||
    !/^[0-9a-f]{64}$/u.test(file.sha256)
  ) {
    throw new MidnightLocalProverError("INVALID_CONFIGURATION");
  }
}

function validateConfiguration(
  configuration: MidnightLocalProverConfiguration,
): void {
  if (
    configuration.parameters.length === 0 ||
    configuration.parameters.length > 32 ||
    configuration.circuits.length > 256
  ) {
    throw new MidnightLocalProverError("INVALID_CONFIGURATION");
  }
  const ks = new Set<number>();
  for (const parameter of configuration.parameters) {
    if (
      !Number.isInteger(parameter.k) ||
      parameter.k < 0 ||
      parameter.k > 255 ||
      ks.has(parameter.k)
    ) {
      throw new MidnightLocalProverError("INVALID_CONFIGURATION");
    }
    ks.add(parameter.k);
    validateFile(parameter.file);
  }
  const locations = new Set<string>();
  for (const circuit of configuration.circuits) {
    if (
      circuit.keyLocation.length === 0 ||
      circuit.keyLocation.length > 1_024 ||
      /[\u0000-\u001f\u007f]/u.test(circuit.keyLocation) ||
      locations.has(circuit.keyLocation)
    ) {
      throw new MidnightLocalProverError("INVALID_CONFIGURATION");
    }
    locations.add(circuit.keyLocation);
    validateFile(circuit.proverKey);
    validateFile(circuit.verifierKey);
    validateFile(circuit.ir);
  }
}

class NativeMidnightLocalProver implements MidnightLocalProver {
  private closed = false;
  readonly proveBatch?: (
    requests: readonly Uint8Array[],
  ) => Promise<Uint8Array[]>;
  readonly setProfiling?: (enabled: boolean) => Promise<void>;
  readonly takeTimings?: () => Promise<string>;

  constructor(private readonly native: NativeLocalProverModule) {
    if (typeof native.proveBatch === "function") {
      this.proveBatch = (requests) => this.executeBatchRequest(requests);
    }
    // Instrumentation, not proving: these stay callable on a closed prover and
    // never take the registry handle, so a drain after the last proof of a
    // session still reports that proof.
    const setProfiling = native.setProfiling?.bind(native);
    if (setProfiling !== undefined) {
      this.setProfiling = (enabled) => nativeCall(() => setProfiling(enabled));
    }
    const takeTimings = native.takeTimings?.bind(native);
    if (takeTimings !== undefined) {
      this.takeTimings = () => nativeCall(() => takeTimings());
    }
  }

  async configure(
    configuration: MidnightLocalProverConfiguration,
  ): Promise<void> {
    if (this.closed) throw new MidnightLocalProverError("STALE_REGISTRY");
    validateConfiguration(configuration);
    await nativeCall(() =>
      this.native.configure(JSON.stringify(configuration)),
    );
  }

  async check(request: Uint8Array): Promise<Uint8Array> {
    this.requireOpen(request);
    return this.executeRequest(request, (copy) => this.native.check(copy));
  }

  async prove(request: Uint8Array): Promise<Uint8Array> {
    this.requireOpen(request);
    return this.executeRequest(request, (copy) => this.native.prove(copy));
  }

  async close(): Promise<void> {
    if (this.closed) return;
    await nativeCall(() => this.native.close());
    this.closed = true;
  }

  private requireOpen(request: Uint8Array): void {
    if (this.closed) throw new MidnightLocalProverError("STALE_REGISTRY");
    if (!(request instanceof Uint8Array) || request.length === 0) {
      throw new MidnightLocalProverError("INVALID_REQUEST");
    }
  }

  private async executeRequest(
    request: Uint8Array,
    operation: (copy: Uint8Array) => Promise<Uint8Array>,
  ): Promise<Uint8Array> {
    const copy = request.slice();
    try {
      return await nativeCall(() => operation(copy));
    } finally {
      copy.fill(0);
    }
  }

  private async executeBatchRequest(
    requests: readonly Uint8Array[],
  ): Promise<Uint8Array[]> {
    for (const request of requests) {
      this.requireOpen(request);
    }
    const copies = requests.map((request) => request.slice());
    const native = this.native;
    try {
      return await nativeCall(() => {
        if (native.proveBatch === undefined) {
          throw new MidnightLocalProverError("NATIVE_INTERNAL");
        }
        return native.proveBatch(copies);
      });
    } finally {
      for (const copy of copies) copy.fill(0);
    }
  }
}

export async function createMidnightLocalProver(
  configuration: MidnightLocalProverConfiguration,
  options: CreateMidnightLocalProverOptions = {},
): Promise<MidnightLocalProver> {
  const prover = new NativeMidnightLocalProver(
    nativeModule(options.nativeModuleLoader),
  );
  await prover.configure(configuration);
  return prover;
}

/**
 * Creates the standard wallet transport with exact Ledger `/check` and `/prove`
 * effects handled locally. Balance-service effects (`proveAndBalance` and
 * `balance`) and every other network effect continue through `config.fetch`.
 */
export function createLocalProverMidnightTransport(
  config: MidnightTransportConfiguration,
  prover: MidnightLocalProver,
): MidnightStandardTransport {
  const proveBatch = prover.proveBatch?.bind(prover);
  return createMidnightTransportWithProofAdapter(config, {
    execute(effect, request) {
      return effect === "check" ? prover.check(request) : prover.prove(request);
    },
    ...(proveBatch === undefined ? {} : { executeProveBatch: proveBatch }),
  });
}
