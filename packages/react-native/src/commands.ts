export type MidnightNetworkId = "preview" | "preprod" | "mainnet";
export type MidnightSyncStream = "shielded" | "unshielded" | "dust";
export type MidnightSyncRequestMode = "standard" | "fast";
export type MidnightWalletType = "shielded" | "unshielded";

export interface MidnightProvingKeyMaterial {
  readonly proverKeyBase64: string;
  readonly verifierKeyBase64: string;
  readonly irBase64: string;
  readonly compression?: "gzip";
}

export interface MidnightDappOutput {
  readonly walletType: MidnightWalletType;
  readonly tokenType: string;
  readonly amount: string;
  readonly receiverAddress: string;
}

export interface MidnightDappInput {
  readonly walletType: MidnightWalletType;
  readonly tokenType: string;
  readonly amount: string;
}

export interface MidnightCommandMap {
  readonly signData: {
    readonly kind: "signData";
    readonly domain: string;
    readonly dataBase64: string;
  };
  readonly createCheckPayload: {
    readonly kind: "createCheckPayload";
    readonly preimageBase64: string;
    readonly irBase64?: string;
  };
  readonly parseCheckResult: {
    readonly kind: "parseCheckResult";
    readonly resultBase64: string;
  };
  readonly createProvingPayload: {
    readonly kind: "createProvingPayload";
    readonly preimageBase64: string;
    readonly bindingInput?: string;
    readonly keyMaterial?: MidnightProvingKeyMaterial;
  };
  readonly canonicalizeTransaction: {
    readonly kind: "canonicalizeTransaction";
    readonly signatureMarker: "signature" | "signature-erased";
    readonly proofMarker: "proof" | "pre-proof" | "no-proof";
    readonly bindingMarker: "binding" | "pre-binding" | "no-binding";
    readonly rawBase64: string;
  };
  readonly createSyncRequest:
    | {
        readonly kind: "createSyncRequest";
        readonly mode?: "standard";
        readonly stream: MidnightSyncStream;
        readonly fromOffset: number;
        readonly limit: number;
      }
    | {
        readonly kind: "createSyncRequest";
        readonly mode: "fast";
        readonly stream: "shielded" | "dust";
        readonly fromOffset: number;
      };
  readonly deriveShieldedMintContext: {
    readonly kind: "deriveShieldedMintContext";
  };
  readonly watchShieldedMint: {
    readonly kind: "watchShieldedMint";
    readonly coinInfoBase64: string;
    readonly expectedOutputIndex: number;
  };
  readonly createShieldedSpentRequest: {
    readonly kind: "createShieldedSpentRequest";
  };
  readonly applyShieldedSpentResponse: {
    readonly kind: "applyShieldedSpentResponse";
    readonly resultBase64: string;
  };
  readonly setShieldedProtocolVersion: {
    readonly kind: "setShieldedProtocolVersion";
    readonly protocolVersion: number;
    readonly syncOffset: number;
  };
  readonly createDustSpendRequest: {
    readonly kind: "createDustSpendRequest";
  };
  readonly createDustCommitmentRequest: {
    readonly kind: "createDustCommitmentRequest";
    readonly rawBase64: string;
    readonly syncOffset: number;
  };
  readonly applyDustSpendResolution: {
    readonly kind: "applyDustSpendResolution";
    readonly rawBase64: string;
    readonly resultBase64: string;
    readonly syncOffset: number;
  };
  readonly transfer: {
    readonly kind: "transfer";
    readonly to: string;
    readonly amount: string;
    readonly tokenType: string;
    readonly walletType: MidnightWalletType;
  };
  readonly dappTransfer: {
    readonly kind: "dappTransfer";
    readonly outputs: readonly MidnightDappOutput[];
  };
  readonly dappIntent: {
    readonly kind: "dappIntent";
    readonly inputs: readonly MidnightDappInput[];
    readonly outputs: readonly MidnightDappOutput[];
  };
  readonly generateDust: {
    readonly kind: "generateDust";
    readonly ledgerParametersBase64: string;
    readonly feeBlocksMargin: number;
    readonly additionalFeeOverhead: string;
  };
  readonly previewBalance: {
    readonly kind: "previewBalance";
    readonly ledgerParametersBase64: string;
    readonly rawBase64: string;
    readonly sealed: boolean;
    readonly feeBlocksMargin: number;
    readonly additionalFeeOverhead: string;
    readonly feeMode: MidnightFeeMode;
  };
  readonly balanceUnsealed: MidnightBalanceCommand<"balanceUnsealed">;
  readonly balanceSealed: MidnightBalanceCommand<"balanceSealed">;
  readonly finalizeUnprovenTransaction: {
    readonly kind: "finalizeUnprovenTransaction";
    readonly rawBase64: string;
    readonly keyMaterial?: Readonly<Record<string, MidnightProvingKeyMaterial>>;
  };
  readonly submitFinalized: {
    readonly kind: "submitFinalized";
    readonly rawBase64: string;
  };
}

interface MidnightBalanceCommand<
  K extends "balanceUnsealed" | "balanceSealed",
> {
  readonly kind: K;
  readonly ledgerParametersBase64: string;
  readonly rawBase64: string;
  readonly feeBlocksMargin: number;
  readonly additionalFeeOverhead: string;
  readonly feeMode: MidnightFeeMode;
  /**
   * The manifest returned by `previewBalance` and shown to the user, passed
   * back verbatim. The runtime replans and refuses to execute unless the fresh
   * plan costs no more than this one.
   */
  readonly approvedManifest: MidnightBalanceManifest;
}

/** Where a transaction's fee comes from. */
export type MidnightFeeMode = "localDust" | "sponsored";

export interface MidnightWalletContribution {
  readonly walletType: MidnightWalletType;
  readonly tokenType: string;
  /** Net amount in atomic units: selected inputs minus returned change. */
  readonly amount: string;
}

export interface MidnightBalanceManifest {
  readonly transactionDigest: string;
  readonly variant: "sealed" | "unsealed";
  readonly contributions: readonly MidnightWalletContribution[];
  readonly change: readonly MidnightWalletContribution[];
  /** Maximum DUST the wallet may spend, or `"sponsored"` when it spends none. */
  readonly dust: string;
  readonly walletStateDigest: string;
}

export interface MidnightBalancePreviewResult {
  readonly manifest: MidnightBalanceManifest;
  readonly manifestDigest: string;
}

export type MidnightCommandKind = keyof MidnightCommandMap;
export type MidnightCommand<K extends MidnightCommandKind> =
  MidnightCommandMap[K] & { readonly kind: K };
export type MidnightRuntimeCommand = MidnightCommandMap[MidnightCommandKind];

export interface MidnightSignatureResult {
  readonly signatureHex: string;
  readonly verifyingKeyHex: string;
}

export interface MidnightPayloadResult {
  readonly payloadBase64: string;
}

export interface MidnightCheckResult {
  readonly values: readonly (string | null)[];
}

export interface MidnightCanonicalTransactionResult {
  readonly transactionBase64: string;
}

export interface MidnightSyncRequestResult {
  readonly stream: MidnightSyncStream;
  readonly fromOffset: number;
  readonly requestBase64: string;
}

export interface MidnightShieldedMintContextResult {
  readonly coinPublicKeyHex: string;
  readonly encryptionPublicKeyHex: string;
  readonly outputIndex: number;
}

export interface MidnightWatchShieldedMintResult {
  readonly outputIndex: number;
}

export interface MidnightShieldedSpentRequestResult {
  readonly requestBase64: string;
  readonly nullifierCount: number;
}

export interface MidnightShieldedSpentResult {
  readonly removedCount: number;
}

export interface MidnightProtocolVersionResult {
  readonly protocolVersion: number;
}

export interface MidnightDustSpendRequestResult {
  readonly requestBase64: string;
  readonly utxoCount: number;
}

export type MidnightDustCommitmentResult =
  | { readonly status: "ahead" | "unchanged" }
  | { readonly status: "rebuild"; readonly requestBase64: string };

export interface MidnightDustResolutionResult {
  readonly status: "applied";
}

export interface MidnightFinalizedTransactionResult {
  readonly transactionBase64: string;
  readonly transactionHash: string;
  readonly ledgerTransactionHash: string;
  readonly identifiers: readonly string[];
  readonly expiresAt?: number;
}

export interface MidnightSubmissionResult {
  readonly transactionHash: string;
  readonly identifiers: readonly string[];
  readonly status: "accepted" | "rejected";
  readonly bodyBase64?: string;
}

export interface MidnightCommandResultMap {
  readonly signData: MidnightSignatureResult;
  readonly createCheckPayload: MidnightPayloadResult;
  readonly parseCheckResult: MidnightCheckResult;
  readonly createProvingPayload: MidnightPayloadResult;
  readonly canonicalizeTransaction: MidnightCanonicalTransactionResult;
  readonly createSyncRequest: MidnightSyncRequestResult;
  readonly deriveShieldedMintContext: MidnightShieldedMintContextResult;
  readonly watchShieldedMint: MidnightWatchShieldedMintResult;
  readonly createShieldedSpentRequest: MidnightShieldedSpentRequestResult;
  readonly applyShieldedSpentResponse: MidnightShieldedSpentResult;
  readonly setShieldedProtocolVersion: MidnightProtocolVersionResult;
  readonly createDustSpendRequest: MidnightDustSpendRequestResult;
  readonly createDustCommitmentRequest: MidnightDustCommitmentResult;
  readonly applyDustSpendResolution: MidnightDustResolutionResult;
  readonly transfer: MidnightFinalizedTransactionResult;
  readonly dappTransfer: MidnightFinalizedTransactionResult;
  readonly dappIntent: MidnightFinalizedTransactionResult;
  readonly generateDust: MidnightFinalizedTransactionResult;
  readonly previewBalance: MidnightBalancePreviewResult;
  readonly balanceUnsealed: MidnightFinalizedTransactionResult;
  readonly balanceSealed: MidnightFinalizedTransactionResult;
  readonly finalizeUnprovenTransaction: MidnightFinalizedTransactionResult;
  readonly submitFinalized: MidnightSubmissionResult;
}

export type MidnightCommandResult<K extends MidnightCommandKind> =
  MidnightCommandResultMap[K];

export const MIDNIGHT_COMMAND_KINDS = [
  "signData",
  "createCheckPayload",
  "parseCheckResult",
  "createProvingPayload",
  "canonicalizeTransaction",
  "createSyncRequest",
  "deriveShieldedMintContext",
  "watchShieldedMint",
  "createShieldedSpentRequest",
  "applyShieldedSpentResponse",
  "setShieldedProtocolVersion",
  "createDustSpendRequest",
  "createDustCommitmentRequest",
  "applyDustSpendResolution",
  "transfer",
  "dappTransfer",
  "dappIntent",
  "generateDust",
  "previewBalance",
  "balanceUnsealed",
  "balanceSealed",
  "finalizeUnprovenTransaction",
  "submitFinalized",
] as const satisfies readonly MidnightCommandKind[];
