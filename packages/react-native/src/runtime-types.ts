import type {
  MidnightCommandKind,
  MidnightCommand,
  MidnightCommandResult,
  MidnightNetworkId,
  MidnightSyncStream,
} from "./commands.js";

export type MidnightEndpointRole = "indexer" | "proof" | "node";
export type MidnightNetworkEffect =
  | "sync"
  | "check"
  | "prove"
  | "proveAndBalance"
  | "balance"
  | "submit"
  | "confirm";

export interface MidnightSessionHandle {
  readonly id: number;
  readonly generation: number;
}

export interface MidnightOperationHandle<
  K extends MidnightCommandKind = MidnightCommandKind,
> {
  readonly id: number;
  readonly generation: number;
  readonly commandKind: K;
}

export interface MidnightCheckpoint {
  readonly version: 1;
  readonly bytes: Uint8Array;
}

export interface MidnightWalletSessionConfig {
  readonly networkId: MidnightNetworkId;
  readonly walletFingerprint: string;
  readonly unshieldedAddress: string;
}

export interface MidnightWalletSessionSecrets {
  readonly nightExternalKey: Uint8Array;
  readonly zswapSeed: Uint8Array;
  readonly dustSeed: Uint8Array;
}

export type MidnightSyncInputStream =
  MidnightSyncStream | "shielded-v2" | "dust-v2";

export interface MidnightSyncBatch {
  readonly stream: MidnightSyncInputStream;
  readonly fromOffset: number;
  readonly toOffset: number;
  readonly payloads: readonly Uint8Array[];
}

export interface MidnightStreamOffset {
  readonly stream: MidnightSyncStream;
  readonly nextOffset: number;
}

export interface MidnightPendingSubmission {
  readonly transactionHash: string;
  readonly identifiers: readonly string[];
  readonly status:
    "awaitingResponse" | "accepted" | "rejected" | "statusUnknown";
}

export interface MidnightDustCoinSnapshot {
  readonly nonce: string;
  readonly generatedNow: string;
  readonly maxCap: string;
  readonly maxCapReachedAt: string | null;
  readonly maturing: boolean;
}

export interface MidnightWalletBalances {
  readonly shieldedBalances: Readonly<Record<string, string>>;
  readonly unshieldedBalances: Readonly<Record<string, string>>;
  readonly totalShielded: string;
  readonly totalUnshielded: string;
  readonly dustBalance: string;
  readonly dustCoins: readonly MidnightDustCoinSnapshot[];
  readonly availableUtxos: number;
  readonly dustGeneratingNight: string;
}

export interface MidnightWalletSnapshot {
  readonly networkId: MidnightNetworkId;
  readonly walletFingerprint: string;
  readonly status: "syncing" | "ready";
  readonly generation: number;
  readonly streamOffsets: readonly MidnightStreamOffset[];
  readonly unshieldedAddress: string;
  readonly shieldedCoinPublicKeyHex: string;
  readonly shieldedEncryptionPublicKeyHex: string;
  readonly dustPublicKey: string;
  readonly balances: MidnightWalletBalances;
  readonly pendingSubmissions: readonly MidnightPendingSubmission[];
}

export interface MidnightApplySyncResult {
  readonly duplicate: boolean;
  readonly snapshot: MidnightWalletSnapshot;
}

export interface MidnightNetworkResult {
  readonly effectId: string;
  readonly outcome: "accepted" | "rejected" | "statusUnknown";
  readonly bodyBase64?: string;
}

export type MidnightOperationStep<K extends MidnightCommandKind> =
  | {
      readonly kind: "progress";
      readonly operation: MidnightOperationHandle<K>;
      readonly phase: string;
      readonly percent?: number;
    }
  | {
      readonly kind: "network";
      readonly operation: MidnightOperationHandle<K>;
      readonly effectId: string;
      readonly effect: MidnightNetworkEffect;
      readonly endpointRole: MidnightEndpointRole;
      readonly bodyBase64: string;
      /**
       * Present only when this round batches 2+ independent proof requests.
       * The singular `effectId`/`effect`/`endpointRole`/`bodyBase64` fields
       * above mirror the first element for backwards compatibility with
       * consumers that only understand a single effect per round.
       */
      readonly effects?: readonly {
        readonly effectId: string;
        readonly effect: "check" | "prove";
        readonly endpointRole: "proof";
        readonly bodyBase64: string;
      }[];
    }
  | {
      readonly kind: "complete";
      readonly operation: MidnightOperationHandle<K> | null;
      readonly result: MidnightCommandResult<K>;
    };

export interface MidnightRuntimeApi {
  openWalletSession(
    config: MidnightWalletSessionConfig,
    secrets: MidnightWalletSessionSecrets,
    checkpoint?: MidnightCheckpoint | null,
  ): Promise<MidnightSessionHandle>;
  applySyncBatch(
    session: MidnightSessionHandle,
    batch: MidnightSyncBatch,
  ): Promise<MidnightApplySyncResult>;
  getWalletSnapshot(
    session: MidnightSessionHandle,
  ): Promise<MidnightWalletSnapshot>;
  exportWalletCheckpoint(
    session: MidnightSessionHandle,
  ): Promise<MidnightCheckpoint>;
  beginCommand<K extends MidnightCommandKind>(
    session: MidnightSessionHandle,
    command: MidnightCommand<K>,
  ): Promise<MidnightOperationStep<K>>;
  resumeOperation<K extends MidnightCommandKind>(
    operation: MidnightOperationHandle<K>,
    networkResult:
      MidnightNetworkResult | readonly MidnightNetworkResult[] | null,
  ): Promise<MidnightOperationStep<K>>;
  cancelOperation(operation: MidnightOperationHandle): Promise<void>;
  closeWalletSession(session: MidnightSessionHandle): Promise<void>;
}

export type MidnightRuntimeStatus =
  "idle" | "opening" | "syncing" | "ready" | "closing" | "error";
