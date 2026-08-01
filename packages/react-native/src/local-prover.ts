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
  | "NATIVE_INTERNAL";

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
]);

export class MidnightLocalProverError extends Error {
  readonly code: MidnightLocalProverErrorCode;

  constructor(code: MidnightLocalProverErrorCode, cause?: unknown) {
    super(code, { cause });
    this.name = "MidnightLocalProverError";
    this.code = code;
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
  close(): Promise<void>;
}

export interface NativeLocalProverModule {
  configure(configurationJson: string): Promise<number>;
  check(request: Uint8Array): Promise<Uint8Array>;
  prove(request: Uint8Array): Promise<Uint8Array>;
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

function errorCode(error: unknown): MidnightLocalProverErrorCode {
  if (error instanceof MidnightLocalProverError) return error.code;
  if (typeof error === "object" && error !== null && "code" in error) {
    const code = Reflect.get(error, "code");
    if (
      typeof code === "string" &&
      ERROR_CODES.has(code as MidnightLocalProverErrorCode)
    ) {
      return code as MidnightLocalProverErrorCode;
    }
  }
  return "NATIVE_INTERNAL";
}

async function nativeCall<T>(operation: () => Promise<T>): Promise<T> {
  try {
    return await operation();
  } catch (error) {
    throw new MidnightLocalProverError(errorCode(error), error);
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

  constructor(private readonly native: NativeLocalProverModule) {}

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
  return createMidnightTransportWithProofAdapter(config, {
    execute(effect, request) {
      return effect === "check" ? prover.check(request) : prover.prove(request);
    },
  });
}
