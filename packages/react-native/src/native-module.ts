import { requireNativeModule } from "expo-modules-core";

export const EXPO_MIDNIGHT_NATIVE_MODULE_NAME = "ExpoMidnightNative";

export interface NativeRuntimeModule {
  openWalletSession(
    configJson: string,
    nightExternalKey: Uint8Array,
    zswapSeed: Uint8Array,
    dustSeed: Uint8Array,
    checkpointBase64: string | null,
  ): Promise<{ readonly id: number; readonly generation: number }>;
  applySyncBatch(
    sessionId: number,
    generation: number,
    stream: string,
    fromOffset: number,
    toOffset: number,
    payloadsBase64: readonly string[],
  ): Promise<string>;
  getWalletSnapshot(sessionId: number, generation: number): Promise<string>;
  exportWalletCheckpoint(
    sessionId: number,
    generation: number,
  ): Promise<string>;
  beginCommand(
    sessionId: number,
    generation: number,
    commandJson: string,
  ): Promise<string>;
  resumeOperation(
    operationId: number,
    generation: number,
    networkResultJson: string | null,
  ): Promise<string>;
  cancelOperation(operationId: number, generation: number): Promise<void>;
  closeWalletSession(sessionId: number, generation: number): Promise<void>;
}

export type NativeRuntimeModuleLoader = () => Partial<NativeRuntimeModule>;

export function loadNativeRuntimeModule(): NativeRuntimeModule {
  return requireNativeModule(
    EXPO_MIDNIGHT_NATIVE_MODULE_NAME,
  ) as NativeRuntimeModule;
}
