import type {
  MidnightCheckpoint,
  MidnightEndpointRole,
} from "./runtime-types.js";

export interface MidnightCheckpointStore {
  load(key: string): Promise<MidnightCheckpoint | null>;
  save(key: string, checkpoint: MidnightCheckpoint): Promise<void>;
  remove(key: string): Promise<void>;
}

export interface MidnightLogEvent {
  readonly level: "debug" | "info" | "warn" | "error";
  readonly event:
    | "session.open"
    | "session.close"
    | "session.pause"
    | "sync.apply"
    | "operation.step"
    | "transport.request"
    | "transport.response"
    | "transport.failure";
  readonly role?: MidnightEndpointRole;
  readonly effect?: string;
  readonly outcome?: string;
  readonly durationMs?: number;
  readonly byteLength?: number;
  readonly errorCode?: string;
}

export interface MidnightLogger {
  write(event: MidnightLogEvent): void;
}

export const silentMidnightLogger: MidnightLogger = {
  write(event) {
    void event;
  },
};

function cloneCheckpoint(checkpoint: MidnightCheckpoint): MidnightCheckpoint {
  return { version: 1, bytes: checkpoint.bytes.slice() };
}

export class InMemoryMidnightCheckpointStore implements MidnightCheckpointStore {
  readonly #values = new Map<string, MidnightCheckpoint>();

  load(key: string): Promise<MidnightCheckpoint | null> {
    const value = this.#values.get(key);
    return Promise.resolve(value ? cloneCheckpoint(value) : null);
  }

  save(key: string, checkpoint: MidnightCheckpoint): Promise<void> {
    this.#values.set(key, cloneCheckpoint(checkpoint));
    return Promise.resolve();
  }

  remove(key: string): Promise<void> {
    this.#values.delete(key);
    return Promise.resolve();
  }
}
