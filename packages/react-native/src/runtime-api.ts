import { decodeBase64, encodeBase64 } from "./base64.js";
import type { MidnightCommandKind, MidnightCommand } from "./commands.js";
import {
  decodeApplySync,
  decodeOperationStep,
  decodeSessionHandle,
  decodeWalletSnapshot,
  parseNativeJson,
} from "./decode.js";
import { MidnightRuntimeError, normalizeMidnightError } from "./errors.js";
import {
  loadNativeRuntimeModule,
  type NativeRuntimeModule,
  type NativeRuntimeModuleLoader,
} from "./native-module.js";
import type {
  MidnightApplySyncResult,
  MidnightCheckpoint,
  MidnightNetworkResult,
  MidnightOperationHandle,
  MidnightOperationStep,
  MidnightRuntimeApi,
  MidnightSessionHandle,
  MidnightSyncBatch,
  MidnightWalletSessionConfig,
  MidnightWalletSessionSecrets,
  MidnightWalletSnapshot,
} from "./runtime-types.js";

function requireModule(loader: NativeRuntimeModuleLoader): NativeRuntimeModule {
  let value: Partial<NativeRuntimeModule>;
  try {
    value = loader();
  } catch {
    throw new MidnightRuntimeError("UNAVAILABLE");
  }
  const methods: readonly (keyof NativeRuntimeModule)[] = [
    "openWalletSession",
    "applySyncBatch",
    "getWalletSnapshot",
    "exportWalletCheckpoint",
    "beginCommand",
    "resumeOperation",
    "cancelOperation",
    "closeWalletSession",
  ];
  for (const method of methods) {
    if (typeof value[method] !== "function") {
      throw new MidnightRuntimeError("UNAVAILABLE");
    }
  }
  return value as NativeRuntimeModule;
}

function validateHandle(
  value: MidnightSessionHandle | MidnightOperationHandle,
): void {
  for (const item of [value.id, value.generation]) {
    if (!Number.isSafeInteger(item) || item < 0) {
      throw new MidnightRuntimeError("INVALID_ARGUMENT");
    }
  }
}

function copySecrets(secrets: MidnightWalletSessionSecrets): {
  readonly nightExternalKey: Uint8Array;
  readonly zswapSeed: Uint8Array;
  readonly dustSeed: Uint8Array;
} {
  const values = [
    secrets.nightExternalKey,
    secrets.zswapSeed,
    secrets.dustSeed,
  ];
  if (values.some((value) => value.length !== 32)) {
    throw new MidnightRuntimeError("INVALID_ARGUMENT");
  }
  return {
    nightExternalKey: secrets.nightExternalKey.slice(),
    zswapSeed: secrets.zswapSeed.slice(),
    dustSeed: secrets.dustSeed.slice(),
  };
}

function wipeSecrets(secrets: {
  readonly nightExternalKey: Uint8Array;
  readonly zswapSeed: Uint8Array;
  readonly dustSeed: Uint8Array;
}): void {
  secrets.nightExternalKey.fill(0);
  secrets.zswapSeed.fill(0);
  secrets.dustSeed.fill(0);
}

async function nativeCall<T>(operation: () => Promise<T>): Promise<T> {
  try {
    return await operation();
  } catch (error) {
    throw normalizeMidnightError(error);
  }
}

async function openWalletSession(
  loader: NativeRuntimeModuleLoader,
  config: MidnightWalletSessionConfig,
  secrets: MidnightWalletSessionSecrets,
  checkpoint: MidnightCheckpoint | null,
): Promise<MidnightSessionHandle> {
  const native = requireModule(loader);
  const owned = copySecrets(secrets);
  try {
    const value = await nativeCall(() =>
      native.openWalletSession(
        JSON.stringify(config),
        owned.nightExternalKey,
        owned.zswapSeed,
        owned.dustSeed,
        checkpoint === null ? null : encodeBase64(checkpoint.bytes),
      ),
    );
    return decodeSessionHandle(value);
  } finally {
    wipeSecrets(owned);
  }
}

function validateSyncBatch(batch: MidnightSyncBatch): void {
  const invalid =
    !Number.isSafeInteger(batch.fromOffset) ||
    !Number.isSafeInteger(batch.toOffset) ||
    batch.fromOffset < 0 ||
    batch.toOffset < batch.fromOffset;
  if (invalid) throw new MidnightRuntimeError("INVALID_ARGUMENT");
}

async function applySyncBatch(
  loader: NativeRuntimeModuleLoader,
  session: MidnightSessionHandle,
  batch: MidnightSyncBatch,
): Promise<MidnightApplySyncResult> {
  validateHandle(session);
  validateSyncBatch(batch);
  const native = requireModule(loader);
  const value = await nativeCall(() =>
    native.applySyncBatch(
      session.id,
      session.generation,
      batch.stream,
      batch.fromOffset,
      batch.toOffset,
      batch.payloads.map(encodeBase64),
    ),
  );
  return decodeApplySync(parseNativeJson(value));
}

async function getWalletSnapshot(
  loader: NativeRuntimeModuleLoader,
  session: MidnightSessionHandle,
): Promise<MidnightWalletSnapshot> {
  validateHandle(session);
  const native = requireModule(loader);
  const value = await nativeCall(() =>
    native.getWalletSnapshot(session.id, session.generation),
  );
  return decodeWalletSnapshot(parseNativeJson(value));
}

async function exportWalletCheckpoint(
  loader: NativeRuntimeModuleLoader,
  session: MidnightSessionHandle,
): Promise<MidnightCheckpoint> {
  validateHandle(session);
  const native = requireModule(loader);
  const value = await nativeCall(() =>
    native.exportWalletCheckpoint(session.id, session.generation),
  );
  return { version: 1, bytes: decodeBase64(value) };
}

async function beginCommand<K extends MidnightCommandKind>(
  loader: NativeRuntimeModuleLoader,
  session: MidnightSessionHandle,
  command: MidnightCommand<K>,
): Promise<MidnightOperationStep<K>> {
  validateHandle(session);
  const native = requireModule(loader);
  const value = await nativeCall(() =>
    native.beginCommand(
      session.id,
      session.generation,
      JSON.stringify(command),
    ),
  );
  return decodeOperationStep<K>(command.kind, parseNativeJson(value));
}

async function resumeOperation<K extends MidnightCommandKind>(
  loader: NativeRuntimeModuleLoader,
  operation: MidnightOperationHandle<K>,
  networkResult: MidnightNetworkResult | null,
): Promise<MidnightOperationStep<K>> {
  validateHandle(operation);
  const native = requireModule(loader);
  const value = await nativeCall(() =>
    native.resumeOperation(
      operation.id,
      operation.generation,
      networkResult === null ? null : JSON.stringify(networkResult),
    ),
  );
  return decodeOperationStep(operation.commandKind, parseNativeJson(value));
}

async function cancelOperation(
  loader: NativeRuntimeModuleLoader,
  operation: MidnightOperationHandle,
): Promise<void> {
  validateHandle(operation);
  const native = requireModule(loader);
  await nativeCall(() =>
    native.cancelOperation(operation.id, operation.generation),
  );
}

async function closeWalletSession(
  loader: NativeRuntimeModuleLoader,
  session: MidnightSessionHandle,
): Promise<void> {
  validateHandle(session);
  const native = requireModule(loader);
  await nativeCall(() =>
    native.closeWalletSession(session.id, session.generation),
  );
}

export function createMidnightRuntimeApi(
  loader: NativeRuntimeModuleLoader = loadNativeRuntimeModule,
): MidnightRuntimeApi {
  return {
    openWalletSession: (config, secrets, checkpoint = null) =>
      openWalletSession(loader, config, secrets, checkpoint),
    applySyncBatch: (session, batch) => applySyncBatch(loader, session, batch),
    getWalletSnapshot: (session) => getWalletSnapshot(loader, session),
    exportWalletCheckpoint: (session) =>
      exportWalletCheckpoint(loader, session),
    beginCommand: (session, command) => beginCommand(loader, session, command),
    resumeOperation: (operation, networkResult) =>
      resumeOperation(loader, operation, networkResult),
    cancelOperation: (operation) => cancelOperation(loader, operation),
    closeWalletSession: (session) => closeWalletSession(loader, session),
  };
}
