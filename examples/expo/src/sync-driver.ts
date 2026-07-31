import {
  MidnightRuntimeError,
  type MidnightRuntimeController,
  type MidnightSessionHandle,
  type MidnightSyncStream,
} from "@1am/midnight-mobile";

import { decodeEvent, planFor, type DecodedEvent } from "./graphql-sync";
import { subscribe, type GraphQLSubscription } from "./graphql-ws";

/**
 * Drives one sync stream: ask the runtime what it needs, subscribe, apply.
 *
 * The runtime never performs this I/O. `createSyncRequest` states an intent
 * (`{stream, fromOffset, limit}`) and `applySyncBatch` consumes the result; how
 * the data is fetched is entirely the host's business, which is why this lives
 * in the example rather than the package.
 *
 * Batches are applied in arrival order with `toOffset` set to the last event id.
 * `applySyncBatch` requires `fromOffset` to equal the session's stored offset,
 * so a batch that fails leaves the offset untouched and is retried from the same
 * position rather than skipped.
 */

export interface StreamProgress {
  readonly stream: MidnightSyncStream;
  readonly applied: number;
  readonly offset: number;
  readonly caughtUp: boolean;
}

export interface SyncOptions {
  readonly indexerWebSocketUrl: string;
  readonly unshieldedAddress: string;
  /** Stop after this many events; keeps a probe run bounded. */
  readonly maxEvents: number;
  /** Idle milliseconds with no event before treating the stream as caught up. */
  readonly idleMs: number;
}

interface Collected {
  readonly events: readonly DecodedEvent[];
  readonly completed: boolean;
}

function collect(
  options: SyncOptions,
  stream: MidnightSyncStream,
  fromOffset: number,
): Promise<Collected> {
  const plan = planFor(stream, fromOffset, options.unshieldedAddress);
  return new Promise<Collected>((resolve, reject) => {
    const events: DecodedEvent[] = [];
    let subscription: GraphQLSubscription | null = null;
    let settled = false;
    let idle: ReturnType<typeof setTimeout>;

    const finish = (completed: boolean) => {
      if (settled) return;
      settled = true;
      clearTimeout(idle);
      subscription?.close();
      resolve({ events, completed });
    };
    const resetIdle = () => {
      clearTimeout(idle);
      idle = setTimeout(() => {
        finish(false);
      }, options.idleMs);
    };
    resetIdle();

    subscribe(options.indexerWebSocketUrl, plan.document, plan.variables, {
      onNext(data) {
        if (settled) return;
        const root: unknown =
          typeof data === "object" && data !== null
            ? (data as Record<string, unknown>)[plan.root]
            : undefined;
        if (root === undefined || root === null) return;
        try {
          events.push(decodeEvent(stream, root));
        } catch (failure: unknown) {
          settled = true;
          clearTimeout(idle);
          subscription?.close();
          reject(
            failure instanceof MidnightRuntimeError
              ? failure
              : new MidnightRuntimeError("INVALID_ARGUMENT"),
          );
          return;
        }
        if (events.length >= options.maxEvents) {
          finish(false);
          return;
        }
        resetIdle();
      },
      onError(error) {
        if (settled) return;
        settled = true;
        clearTimeout(idle);
        reject(error);
      },
      onComplete() {
        finish(true);
      },
    }).then(
      (handle) => {
        subscription = handle;
        if (settled) handle.close();
      },
      (failure: unknown) => {
        if (settled) return;
        settled = true;
        clearTimeout(idle);
        reject(
          failure instanceof MidnightRuntimeError
            ? failure
            : new MidnightRuntimeError("TRANSPORT_ERROR"),
        );
      },
    );
  });
}

async function applyEvents(
  controller: MidnightRuntimeController,
  session: MidnightSessionHandle,
  stream: MidnightSyncStream,
  fromOffset: number,
  events: readonly DecodedEvent[],
): Promise<number> {
  const last = events.at(-1);
  if (last === undefined) return fromOffset;
  await controller.applySyncBatch(session, {
    stream,
    fromOffset,
    toOffset: last.id,
    payloads: events.map((event) => event.payload),
  });
  return last.id;
}

/**
 * Syncs one stream and marks it caught up.
 *
 * The terminal marker is an empty batch with `fromOffset === toOffset`, which
 * the package translates into the runtime's `<stream>-tip` signal. Sending it
 * before the stream is actually drained would report a wallet as synced when it
 * is not, so it is only sent when the collector saw no further events.
 */
export async function syncStream(
  controller: MidnightRuntimeController,
  session: MidnightSessionHandle,
  stream: MidnightSyncStream,
  options: SyncOptions,
): Promise<StreamProgress> {
  const request = await controller.runCommand(session, {
    kind: "createSyncRequest",
    stream,
    fromOffset: 0,
    limit: options.maxEvents,
  });
  let offset = request.fromOffset;
  const collected = await collect(options, stream, offset);
  offset = await applyEvents(
    controller,
    session,
    stream,
    offset,
    collected.events,
  );
  const drained = collected.events.length < options.maxEvents;
  if (drained) {
    await controller.applySyncBatch(session, {
      stream,
      fromOffset: offset,
      toOffset: offset,
      payloads: [],
    });
  }
  return {
    stream,
    applied: collected.events.length,
    offset,
    caughtUp: drained,
  };
}
