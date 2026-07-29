import type {
  MidnightCommandKind,
  MidnightCommand,
  MidnightCommandResult,
} from "./commands.js";
import { MidnightRuntimeError, normalizeMidnightError } from "./errors.js";
import {
  silentMidnightLogger,
  type MidnightCheckpointStore,
  type MidnightLogger,
} from "./host.js";
import type {
  MidnightApplySyncResult,
  MidnightCheckpoint,
  MidnightOperationHandle,
  MidnightOperationStep,
  MidnightRuntimeApi,
  MidnightRuntimeStatus,
  MidnightSessionHandle,
  MidnightSyncBatch,
  MidnightWalletSessionConfig,
  MidnightWalletSessionSecrets,
  MidnightWalletSnapshot,
} from "./runtime-types.js";
import type {
  MidnightRunCommandOptions,
  MidnightStandardTransport,
} from "./transport.js";

interface SessionRecord {
  readonly config: MidnightWalletSessionConfig;
  readonly handle: MidnightSessionHandle;
  readonly aborters: Set<AbortController>;
  readonly operations: Map<string, MidnightOperationHandle>;
}

export interface MidnightRuntimeControllerOptions {
  readonly api: MidnightRuntimeApi;
  readonly transport: MidnightStandardTransport;
  readonly checkpointStore?: MidnightCheckpointStore;
  readonly logger?: MidnightLogger;
}

export class MidnightRuntimeController {
  readonly #api: MidnightRuntimeApi;
  readonly #transport: MidnightStandardTransport;
  readonly #store: MidnightCheckpointStore | undefined;
  readonly #logger: MidnightLogger;
  readonly #sessions = new Map<string, SessionRecord>();
  readonly #listeners = new Set<(status: MidnightRuntimeStatus) => void>();
  #status: MidnightRuntimeStatus = "idle";

  constructor(options: MidnightRuntimeControllerOptions) {
    this.#api = options.api;
    this.#transport = options.transport;
    this.#store = options.checkpointStore;
    this.#logger = options.logger ?? silentMidnightLogger;
  }

  get status(): MidnightRuntimeStatus {
    return this.#status;
  }

  subscribe(listener: (status: MidnightRuntimeStatus) => void): () => void {
    this.#listeners.add(listener);
    return () => {
      this.#listeners.delete(listener);
    };
  }

  async openWalletSession(
    config: MidnightWalletSessionConfig,
    secrets: MidnightWalletSessionSecrets,
    checkpoint?: MidnightCheckpoint | null,
  ): Promise<MidnightSessionHandle> {
    this.#setStatus("opening");
    try {
      const restored =
        checkpoint === undefined
          ? ((await this.#store?.load(this.#checkpointKey(config))) ?? null)
          : checkpoint;
      const handle = await this.#api.openWalletSession(
        config,
        secrets,
        restored,
      );
      this.#sessions.set(this.#sessionKey(handle), {
        config,
        handle,
        aborters: new Set(),
        operations: new Map(),
      });
      const snapshot = await this.#api.getWalletSnapshot(handle);
      this.#setStatus(snapshot.status);
      this.#logger.write({ level: "info", event: "session.open" });
      return handle;
    } catch (error) {
      this.#setStatus("error");
      throw normalizeMidnightError(error);
    }
  }

  async applySyncBatch(
    session: MidnightSessionHandle,
    batch: MidnightSyncBatch,
  ): Promise<MidnightApplySyncResult> {
    const record = this.#record(session);
    this.#setStatus("syncing");
    const result = await this.#api.applySyncBatch(session, batch);
    if (!result.duplicate) await this.#persist(record);
    this.#setStatus(result.snapshot.status);
    this.#logger.write({
      level: "debug",
      event: "sync.apply",
      byteLength: batch.payloads.reduce(
        (total, payload) => total + payload.length,
        0,
      ),
    });
    return result;
  }

  getWalletSnapshot(
    session: MidnightSessionHandle,
  ): Promise<MidnightWalletSnapshot> {
    this.#record(session);
    return this.#api.getWalletSnapshot(session);
  }

  async runCommand<K extends MidnightCommandKind>(
    session: MidnightSessionHandle,
    command: MidnightCommand<K>,
    options?: MidnightRunCommandOptions<K>,
  ): Promise<MidnightCommandResult<K>> {
    const record = this.#record(session);
    const aborter = new AbortController();
    record.aborters.add(aborter);
    const relayAbort = () => {
      aborter.abort();
    };
    options?.signal?.addEventListener("abort", relayAbort, { once: true });
    try {
      return await this.#transport.runCommand(this.#api, session, command, {
        signal: aborter.signal,
        onStep: async (step) => {
          this.#trackStep(record, step);
          await this.#persist(record);
          await options?.onStep?.(step);
        },
      });
    } finally {
      record.aborters.delete(aborter);
      options?.signal?.removeEventListener("abort", relayAbort);
    }
  }

  async pause(): Promise<void> {
    this.#logger.write({ level: "info", event: "session.pause" });
    await Promise.all(
      [...this.#sessions.values()].map(async (record) => {
        this.#abort(record);
        await this.#cancel(record);
        await this.#persist(record);
      }),
    );
  }

  async refresh(): Promise<void> {
    const snapshots = await Promise.all(
      [...this.#sessions.values()].map((record) =>
        this.#api.getWalletSnapshot(record.handle),
      ),
    );
    if (snapshots.length === 0) this.#setStatus("idle");
    else {
      this.#setStatus(
        snapshots.every((snapshot) => snapshot.status === "ready")
          ? "ready"
          : "syncing",
      );
    }
  }

  async closeWalletSession(session: MidnightSessionHandle): Promise<void> {
    const record = this.#record(session);
    this.#setStatus("closing");
    this.#abort(record);
    await this.#cancel(record);
    let persistenceError: MidnightRuntimeError | null = null;
    try {
      await this.#persist(record);
    } catch (error) {
      persistenceError = normalizeMidnightError(error);
    }
    try {
      await this.#api.closeWalletSession(session);
    } finally {
      this.#sessions.delete(this.#sessionKey(session));
      this.#setStatus(this.#sessions.size === 0 ? "idle" : "syncing");
    }
    this.#logger.write({ level: "info", event: "session.close" });
    if (persistenceError !== null) throw persistenceError;
  }

  async dispose(): Promise<void> {
    const sessions = [...this.#sessions.values()].map(
      (record) => record.handle,
    );
    const results = await Promise.allSettled(
      sessions.map((session) => this.closeWalletSession(session)),
    );
    if (results.some((result) => result.status === "rejected")) {
      throw new MidnightRuntimeError("NATIVE_INTERNAL");
    }
  }

  #setStatus(status: MidnightRuntimeStatus): void {
    this.#status = status;
    for (const listener of this.#listeners) listener(status);
  }

  #sessionKey(handle: MidnightSessionHandle): string {
    return `${String(handle.generation)}:${String(handle.id)}`;
  }

  #checkpointKey(config: MidnightWalletSessionConfig): string {
    return `${config.networkId}:${config.walletFingerprint}`;
  }

  #record(handle: MidnightSessionHandle): SessionRecord {
    const record = this.#sessions.get(this.#sessionKey(handle));
    if (record === undefined) throw new MidnightRuntimeError("STALE_SESSION");
    return record;
  }

  #trackStep<K extends MidnightCommandKind>(
    record: SessionRecord,
    step: MidnightOperationStep<K>,
  ): void {
    if (step.kind === "complete") {
      if (step.operation !== null) {
        record.operations.delete(this.#sessionKey(step.operation));
      }
    } else {
      record.operations.set(this.#sessionKey(step.operation), step.operation);
    }
    this.#logger.write({
      level: "debug",
      event: "operation.step",
      ...(step.kind === "network"
        ? { role: step.endpointRole, effect: step.effect }
        : {}),
    });
  }

  #abort(record: SessionRecord): void {
    for (const aborter of record.aborters) aborter.abort();
    record.aborters.clear();
  }

  async #cancel(record: SessionRecord): Promise<void> {
    const operations = [...record.operations.values()];
    record.operations.clear();
    await Promise.all(
      operations.map((operation) =>
        this.#api.cancelOperation(operation).catch(() => undefined),
      ),
    );
  }

  async #persist(record: SessionRecord): Promise<void> {
    if (this.#store === undefined) return;
    const checkpoint = await this.#api.exportWalletCheckpoint(record.handle);
    await this.#store.save(this.#checkpointKey(record.config), checkpoint);
  }
}
