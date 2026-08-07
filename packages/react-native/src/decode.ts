import type {
  MidnightCommandKind,
  MidnightCommandResultMap,
  MidnightDustCommitmentResult,
  MidnightFinalizedTransactionResult,
  MidnightSyncStream,
} from "./commands.js";
import { decodeBase64 } from "./base64.js";
import { MidnightRuntimeError } from "./errors.js";
import type {
  MidnightApplySyncResult,
  MidnightEndpointRole,
  MidnightNetworkEffect,
  MidnightOperationHandle,
  MidnightOperationStep,
  MidnightPendingSubmission,
  MidnightSessionHandle,
  MidnightWalletBalances,
  MidnightWalletSnapshot,
} from "./runtime-types.js";

type RecordValue = Record<string, unknown>;

function invalid(): never {
  throw new MidnightRuntimeError("NATIVE_INTERNAL");
}

function record(value: unknown): RecordValue {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return invalid();
  }
  return value as RecordValue;
}

function text(value: unknown): string {
  return typeof value === "string" ? value : invalid();
}

function safeInteger(value: unknown): number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0
    ? value
    : invalid();
}

function booleanValue(value: unknown): boolean {
  return typeof value === "boolean" ? value : invalid();
}

function canonicalDecimal(value: unknown): string {
  const result = text(value);
  return /^(?:0|[1-9][0-9]*)$/.test(result) ? result : invalid();
}

function lowercaseHex(value: unknown, bytes?: number): string {
  const result = text(value);
  const pattern =
    bytes === undefined
      ? /^(?:[0-9a-f]{2})+$/
      : new RegExp(`^[0-9a-f]{${String(bytes * 2)}}$`);
  return pattern.test(result) ? result : invalid();
}

function base64(value: unknown): string {
  const result = text(value);
  try {
    decodeBase64(result);
    return result;
  } catch {
    return invalid();
  }
}

function stringArray(
  value: unknown,
  decode: (item: unknown) => string,
): string[] {
  return Array.isArray(value) ? value.map((item) => decode(item)) : invalid();
}

function optionalText(source: RecordValue, key: string): string | undefined {
  const value = source[key];
  return value === undefined ? undefined : text(value);
}

function optionalInteger(source: RecordValue, key: string): number | undefined {
  const value = source[key];
  return value === undefined ? undefined : safeInteger(value);
}

function decodeSyncStream(value: unknown): MidnightSyncStream {
  return value === "shielded" || value === "unshielded" || value === "dust"
    ? value
    : invalid();
}

function decodeHandle(value: unknown): MidnightSessionHandle {
  const source = record(value);
  return {
    id: safeInteger(source.id),
    generation: safeInteger(source.generation),
  };
}

function decodeOperationHandle<K extends MidnightCommandKind>(
  kind: K,
  value: unknown,
): MidnightOperationHandle<K> {
  const handle = decodeHandle(value);
  return { ...handle, commandKind: kind };
}

function decodeBalances(value: unknown): MidnightWalletBalances {
  const source = record(value);
  const dustCoins = Array.isArray(source.dustCoins)
    ? source.dustCoins.map((entry) => {
        const coin = record(entry);
        const reached = coin.maxCapReachedAt;
        return {
          nonce: canonicalDecimal(coin.nonce),
          generatedNow: canonicalDecimal(coin.generatedNow),
          maxCap: canonicalDecimal(coin.maxCap),
          maxCapReachedAt: reached === null ? null : text(reached),
          maturing: booleanValue(coin.maturing),
        };
      })
    : invalid();
  return {
    shieldedBalances: decodeBalanceMap(source.shieldedBalances),
    unshieldedBalances: decodeBalanceMap(source.unshieldedBalances),
    totalShielded: canonicalDecimal(source.totalShielded),
    totalUnshielded: canonicalDecimal(source.totalUnshielded),
    dustBalance: canonicalDecimal(source.dustBalance),
    dustCoins,
    availableUtxos: safeInteger(source.availableUtxos),
    dustGeneratingNight: canonicalDecimal(source.dustGeneratingNight),
  };
}

function decodeBalanceMap(value: unknown): Readonly<Record<string, string>> {
  const source = record(value);
  const result: Record<string, string> = {};
  for (const [key, amount] of Object.entries(source)) {
    result[lowercaseHex(key)] = canonicalDecimal(amount);
  }
  return result;
}

function decodePending(value: unknown): MidnightPendingSubmission[] {
  if (!Array.isArray(value)) return invalid();
  return value.map((entry) => {
    const source = record(entry);
    const status = source.status;
    if (
      status !== "awaitingResponse" &&
      status !== "accepted" &&
      status !== "rejected" &&
      status !== "statusUnknown"
    ) {
      return invalid();
    }
    return {
      transactionHash: lowercaseHex(source.transactionHash, 32),
      identifiers: stringArray(source.identifiers, lowercaseHex),
      status,
    };
  });
}

export function decodeWalletSnapshot(value: unknown): MidnightWalletSnapshot {
  const source = record(value);
  const networkId = source.networkId;
  const status = source.status;
  if (
    networkId !== "preview" &&
    networkId !== "preprod" &&
    networkId !== "mainnet"
  ) {
    return invalid();
  }
  if (status !== "syncing" && status !== "ready") return invalid();
  if (!Array.isArray(source.streamOffsets)) return invalid();
  return {
    networkId,
    walletFingerprint: text(source.walletFingerprint),
    status,
    generation: safeInteger(source.generation),
    streamOffsets: source.streamOffsets.map((entry) => {
      const offset = record(entry);
      return {
        stream: decodeSyncStream(offset.stream),
        nextOffset: safeInteger(offset.nextOffset),
      };
    }),
    unshieldedAddress: text(source.unshieldedAddress),
    shieldedCoinPublicKeyHex: lowercaseHex(source.shieldedCoinPublicKeyHex, 32),
    shieldedEncryptionPublicKeyHex: lowercaseHex(
      source.shieldedEncryptionPublicKeyHex,
      32,
    ),
    dustPublicKey: canonicalDecimal(source.dustPublicKey),
    balances: decodeBalances(source.balances),
    pendingSubmissions: decodePending(source.pendingSubmissions),
  };
}

function decodeFinalized(value: unknown): MidnightFinalizedTransactionResult {
  const source = record(value);
  const expiresAt = optionalInteger(source, "expiresAt");
  return {
    transactionBase64: base64(source.transactionBase64),
    transactionHash: lowercaseHex(source.transactionHash, 32),
    ledgerTransactionHash: lowercaseHex(source.ledgerTransactionHash, 32),
    identifiers: stringArray(source.identifiers, lowercaseHex),
    ...(expiresAt === undefined ? {} : { expiresAt }),
  };
}

function decodeDustCommitment(value: unknown): MidnightDustCommitmentResult {
  const source = record(value);
  if (source.status === "ahead" || source.status === "unchanged") {
    return { status: source.status };
  }
  return source.status === "rebuild"
    ? { status: "rebuild", requestBase64: base64(source.requestBase64) }
    : invalid();
}

const RESULT_DECODERS: {
  readonly [K in MidnightCommandKind]: (
    value: unknown,
  ) => MidnightCommandResultMap[K];
} = {
  signData(value) {
    const source = record(value);
    return {
      signatureHex: lowercaseHex(source.signatureHex, 64),
      verifyingKeyHex: lowercaseHex(source.verifyingKeyHex, 32),
    };
  },
  createCheckPayload: decodePayload,
  parseCheckResult(value) {
    const source = record(value);
    if (!Array.isArray(source.values)) return invalid();
    return {
      values: source.values.map((item) =>
        item === null ? null : canonicalDecimal(item),
      ),
    };
  },
  createProvingPayload: decodePayload,
  canonicalizeTransaction(value) {
    return {
      transactionBase64: base64(record(value).transactionBase64),
    };
  },
  createSyncRequest(value) {
    const source = record(value);
    return {
      stream: decodeSyncStream(source.stream),
      fromOffset: safeInteger(source.fromOffset),
      requestBase64: base64(source.requestBase64),
    };
  },
  deriveShieldedMintContext(value) {
    const source = record(value);
    return {
      coinPublicKeyHex: lowercaseHex(source.coinPublicKeyHex, 32),
      encryptionPublicKeyHex: lowercaseHex(source.encryptionPublicKeyHex, 32),
      outputIndex: safeInteger(source.outputIndex),
    };
  },
  watchShieldedMint(value) {
    return { outputIndex: safeInteger(record(value).outputIndex) };
  },
  createShieldedSpentRequest(value) {
    const source = record(value);
    return {
      requestBase64: base64(source.requestBase64),
      nullifierCount: safeInteger(source.nullifierCount),
    };
  },
  applyShieldedSpentResponse(value) {
    return { removedCount: safeInteger(record(value).removedCount) };
  },
  setShieldedProtocolVersion(value) {
    return { protocolVersion: safeInteger(record(value).protocolVersion) };
  },
  createDustSpendRequest(value) {
    const source = record(value);
    return {
      requestBase64: base64(source.requestBase64),
      utxoCount: safeInteger(source.utxoCount),
    };
  },
  createDustCommitmentRequest: decodeDustCommitment,
  applyDustSpendResolution(value) {
    return record(value).status === "applied"
      ? { status: "applied" }
      : invalid();
  },
  transfer: decodeFinalized,
  dappTransfer: decodeFinalized,
  dappIntent: decodeFinalized,
  generateDust: decodeFinalized,
  balanceUnsealed: decodeFinalized,
  balanceSealed: decodeFinalized,
  finalizeUnprovenTransaction: decodeFinalized,
  submitFinalized(value) {
    const source = record(value);
    const status = source.status;
    if (status !== "accepted" && status !== "rejected") return invalid();
    const bodyBase64 = optionalText(source, "bodyBase64");
    return {
      transactionHash: lowercaseHex(source.transactionHash, 32),
      identifiers: stringArray(source.identifiers, lowercaseHex),
      status,
      ...(bodyBase64 === undefined ? {} : { bodyBase64: base64(bodyBase64) }),
    };
  },
};

function decodePayload(value: unknown): { readonly payloadBase64: string } {
  return { payloadBase64: base64(record(value).payloadBase64) };
}

export function decodeCommandResult<K extends MidnightCommandKind>(
  kind: K,
  value: unknown,
): MidnightCommandResultMap[K] {
  return RESULT_DECODERS[kind](value);
}

/**
 * Decodes the additive batched-effects list. Absent means a single-effect round, which
 * stays wire-identical to the pre-batching protocol. Present but malformed is a hard
 * failure rather than a silent downgrade to the first effect: under-filling a batch
 * would strand the runtime waiting on requests nobody ran.
 */
function decodeBatchedEffects(value: unknown):
  | readonly {
      readonly effectId: string;
      readonly effect: "check" | "prove";
      readonly endpointRole: "proof";
      readonly bodyBase64: string;
    }[]
  | undefined {
  if (value === undefined || value === null) return undefined;
  if (!Array.isArray(value) || value.length < 2) return invalid();
  return value.map((entry) => {
    const effect = record(entry);
    const kind = decodeEffect(effect.effect);
    if (kind !== "check" && kind !== "prove") return invalid();
    if (decodeRole(effect.endpointRole) !== "proof") return invalid();
    return {
      effectId: text(effect.effectId),
      effect: kind,
      endpointRole: "proof" as const,
      bodyBase64: base64(effect.bodyBase64),
    };
  });
}

function decodeRole(value: unknown): MidnightEndpointRole {
  return value === "indexer" || value === "proof" || value === "node"
    ? value
    : invalid();
}

function decodeEffect(value: unknown): MidnightNetworkEffect {
  return value === "sync" ||
    value === "check" ||
    value === "prove" ||
    value === "proveAndBalance" ||
    value === "balance" ||
    value === "submit" ||
    value === "confirm"
    ? value
    : invalid();
}

export function decodeOperationStep<K extends MidnightCommandKind>(
  commandKind: K,
  value: unknown,
): MidnightOperationStep<K> {
  const source = record(value);
  const operation =
    source.operation === null
      ? null
      : decodeOperationHandle(commandKind, source.operation);
  if (source.kind === "complete") {
    return {
      kind: "complete",
      operation,
      result: decodeCommandResult(commandKind, source.result),
    };
  }
  if (operation === null) return invalid();
  if (source.kind === "progress") {
    const percent = optionalInteger(source, "percent");
    return {
      kind: "progress",
      operation,
      phase: text(source.phase),
      ...(percent === undefined ? {} : { percent }),
    };
  }
  if (source.kind === "network") {
    const effects = decodeBatchedEffects(source.effects);
    return {
      kind: "network",
      operation,
      effectId: text(source.effectId),
      effect: decodeEffect(source.effect),
      endpointRole: decodeRole(source.endpointRole),
      bodyBase64: base64(source.bodyBase64),
      ...(effects === undefined ? {} : { effects }),
    };
  }
  return invalid();
}

export function decodeSessionHandle(value: unknown): MidnightSessionHandle {
  return decodeHandle(value);
}

export function decodeApplySync(value: unknown): MidnightApplySyncResult {
  const source = record(value);
  return {
    duplicate: booleanValue(source.duplicate),
    snapshot: decodeWalletSnapshot(source.snapshot),
  };
}

export function parseNativeJson(value: string): unknown {
  try {
    return JSON.parse(value) as unknown;
  } catch {
    return invalid();
  }
}
