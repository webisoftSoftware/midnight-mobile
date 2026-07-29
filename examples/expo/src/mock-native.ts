import type {
  MidnightNetworkId,
  MidnightSyncStream,
  NativeRuntimeModule,
} from "@1am/midnight-mobile";

import { decodeMockBase64, encodeMockBase64 } from "./mock-base64";

interface MockSessionConfiguration {
  readonly networkId: MidnightNetworkId;
  readonly walletFingerprint: string;
  readonly unshieldedAddress: string;
}

interface MockOperation {
  readonly id: number;
  readonly kind: "transfer" | "submitFinalized" | "createProvingPayload";
  stage: "progress" | "network";
}

type MockOffsets = Record<MidnightSyncStream, number>;

function parseRecord(value: string): Record<string, unknown> {
  const parsed: unknown = JSON.parse(value);
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    throw new Error("INVALID_ARGUMENT");
  }
  return parsed as Record<string, unknown>;
}

function parseConfiguration(value: string): MockSessionConfiguration {
  const parsed = parseRecord(value);
  const networkId = parsed.networkId;
  if (
    networkId !== "preview" &&
    networkId !== "preprod" &&
    networkId !== "mainnet"
  ) {
    throw new Error("INVALID_ARGUMENT");
  }
  if (
    typeof parsed.walletFingerprint !== "string" ||
    typeof parsed.unshieldedAddress !== "string"
  ) {
    throw new Error("INVALID_ARGUMENT");
  }
  return {
    networkId,
    walletFingerprint: parsed.walletFingerprint,
    unshieldedAddress: parsed.unshieldedAddress,
  };
}

function commandKind(
  value: string,
): "transfer" | "submitFinalized" | "createProvingPayload" {
  const kind = parseRecord(value).kind;
  if (
    kind !== "transfer" &&
    kind !== "submitFinalized" &&
    kind !== "createProvingPayload"
  ) {
    throw new Error("INVALID_ARGUMENT");
  }
  return kind;
}

function networkOutcome(value: string | null): string {
  if (value === null) return "progress";
  const outcome = parseRecord(value).outcome;
  return typeof outcome === "string" ? outcome : "invalid";
}

export class MockNativeRuntimeModule implements NativeRuntimeModule {
  readonly #generation = 1;
  readonly #sessionId = 1;
  readonly #offsets: MockOffsets = {
    shielded: 0,
    unshielded: 0,
    dust: 0,
  };
  readonly #operations = new Map<number, MockOperation>();
  #configuration: MockSessionConfiguration | null = null;
  #nextOperationId = 1;

  openWalletSession(
    configJson: string,
    nightExternalKey: Uint8Array,
    zswapSeed: Uint8Array,
    dustSeed: Uint8Array,
    checkpointBase64: string | null,
  ): Promise<{ readonly id: number; readonly generation: number }> {
    void nightExternalKey;
    void zswapSeed;
    void dustSeed;
    this.#configuration = parseConfiguration(configJson);
    if (checkpointBase64 !== null) this.#restore(checkpointBase64);
    return Promise.resolve({
      id: this.#sessionId,
      generation: this.#generation,
    });
  }

  applySyncBatch(
    sessionId: number,
    generation: number,
    stream: string,
    fromOffset: number,
    toOffset: number,
    payloadsBase64: readonly string[],
  ): Promise<string> {
    this.#validateSession(sessionId, generation);
    void payloadsBase64;
    if (stream !== "shielded" && stream !== "unshielded" && stream !== "dust") {
      return Promise.reject(new Error("INVALID_ARGUMENT"));
    }
    const duplicate = this.#offsets[stream] === toOffset;
    if (!duplicate && this.#offsets[stream] !== fromOffset) {
      return Promise.reject(new Error("SYNC_GAP"));
    }
    this.#offsets[stream] = toOffset;
    return Promise.resolve(
      JSON.stringify({ duplicate, snapshot: this.#snapshot() }),
    );
  }

  getWalletSnapshot(sessionId: number, generation: number): Promise<string> {
    this.#validateSession(sessionId, generation);
    return Promise.resolve(JSON.stringify(this.#snapshot()));
  }

  exportWalletCheckpoint(
    sessionId: number,
    generation: number,
  ): Promise<string> {
    this.#validateSession(sessionId, generation);
    const bytes = Uint8Array.of(
      this.#offsets.shielded,
      this.#offsets.unshielded,
      this.#offsets.dust,
    );
    return Promise.resolve(encodeMockBase64(bytes));
  }

  beginCommand(
    sessionId: number,
    generation: number,
    commandJson: string,
  ): Promise<string> {
    this.#validateSession(sessionId, generation);
    const kind = commandKind(commandJson);
    const operation = {
      id: this.#nextOperationId,
      kind,
      stage: kind === "transfer" ? "progress" : "network",
    } satisfies MockOperation;
    this.#nextOperationId += 1;
    this.#operations.set(operation.id, operation);
    return Promise.resolve(JSON.stringify(this.#step(operation)));
  }

  resumeOperation(
    operationId: number,
    generation: number,
    networkResultJson: string | null,
  ): Promise<string> {
    if (generation !== this.#generation) {
      return Promise.reject(new Error("STALE_SESSION"));
    }
    const operation = this.#operations.get(operationId);
    if (operation === undefined) {
      return Promise.reject(new Error("CANCELLED"));
    }
    if (operation.stage === "progress") {
      operation.stage = "network";
      return Promise.resolve(JSON.stringify(this.#step(operation)));
    }
    this.#operations.delete(operationId);
    return Promise.resolve(
      JSON.stringify(
        this.#complete(operation, networkOutcome(networkResultJson)),
      ),
    );
  }

  cancelOperation(operationId: number, generation: number): Promise<void> {
    if (generation === this.#generation) this.#operations.delete(operationId);
    return Promise.resolve();
  }

  closeWalletSession(sessionId: number, generation: number): Promise<void> {
    this.#validateSession(sessionId, generation);
    this.#operations.clear();
    this.#configuration = null;
    return Promise.resolve();
  }

  #step(operation: MockOperation): object {
    const handle = { id: operation.id, generation: this.#generation };
    if (operation.stage === "progress") {
      return {
        kind: "progress",
        operation: handle,
        phase: "constructing",
        percent: 25,
      };
    }
    const submit = operation.kind === "submitFinalized";
    return {
      kind: "network",
      operation: handle,
      effectId: `mock-${String(operation.id)}`,
      effect: submit ? "submit" : "prove",
      endpointRole: submit ? "node" : "proof",
      bodyBase64: encodeMockBase64(Uint8Array.of(operation.id)),
    };
  }

  #complete(operation: MockOperation, outcome: string): object {
    const handle = { id: operation.id, generation: this.#generation };
    if (outcome === "statusUnknown") {
      throw new Error("SUBMISSION_STATUS_UNKNOWN");
    }
    const result =
      operation.kind === "transfer"
        ? this.#finalizedTransaction()
        : operation.kind === "submitFinalized"
          ? {
              transactionHash: "11".repeat(32),
              identifiers: ["33".repeat(32)],
              status: outcome === "accepted" ? "accepted" : "rejected",
              bodyBase64: encodeMockBase64(Uint8Array.of(7, 8, 9)),
            }
          : { payloadBase64: encodeMockBase64(Uint8Array.of(4, 5, 6)) };
    return { kind: "complete", operation: handle, result };
  }

  #finalizedTransaction(): object {
    return {
      transactionBase64: encodeMockBase64(Uint8Array.of(1, 2, 3)),
      transactionHash: "11".repeat(32),
      ledgerTransactionHash: "22".repeat(32),
      identifiers: ["33".repeat(32)],
      expiresAt: 1_000,
    };
  }

  #snapshot(): object {
    const configuration = this.#configuration;
    if (configuration === null) throw new Error("STALE_SESSION");
    const ready = Object.values(this.#offsets).every((offset) => offset > 0);
    return {
      ...configuration,
      status: ready ? "ready" : "syncing",
      generation: this.#generation,
      streamOffsets: Object.entries(this.#offsets).map(
        ([stream, nextOffset]) => ({
          stream,
          nextOffset,
        }),
      ),
      shieldedCoinPublicKeyHex: "44".repeat(32),
      shieldedEncryptionPublicKeyHex: "55".repeat(32),
      dustPublicKey: "7",
      balances: {
        shieldedBalances: { ["66".repeat(32)]: ready ? "120" : "0" },
        unshieldedBalances: { ["66".repeat(32)]: ready ? "30" : "0" },
        totalShielded: ready ? "120" : "0",
        totalUnshielded: ready ? "30" : "0",
        dustBalance: ready ? "9" : "0",
        dustCoins: [],
        availableUtxos: ready ? 2 : 0,
        dustGeneratingNight: ready ? "15" : "0",
      },
      pendingSubmissions: [],
    };
  }

  #restore(checkpointBase64: string): void {
    const bytes = decodeMockBase64(checkpointBase64);
    if (bytes.length !== 3) throw new Error("STATE_INCOMPATIBLE");
    this.#offsets.shielded = bytes[0] ?? 0;
    this.#offsets.unshielded = bytes[1] ?? 0;
    this.#offsets.dust = bytes[2] ?? 0;
  }

  #validateSession(sessionId: number, generation: number): void {
    if (
      sessionId !== this.#sessionId ||
      generation !== this.#generation ||
      this.#configuration === null
    ) {
      throw new Error("STALE_SESSION");
    }
  }
}
