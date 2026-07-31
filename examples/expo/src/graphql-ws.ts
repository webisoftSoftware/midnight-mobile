import { MidnightRuntimeError } from "@1am/midnight-mobile";

import {
  createReactNativeWebSocketFactory,
  GRAPHQL_WS_SUBPROTOCOL,
} from "./live-services";

/**
 * Minimal `graphql-transport-ws` client for indexer subscriptions.
 *
 * Only what the sync client needs: connection_init/ack, one subscribe, and
 * next/error/complete. Verified against the public Midnight preview indexer,
 * which rejects the handshake with 400 unless the subprotocol is negotiated.
 */

interface ServerMessage {
  readonly type: string;
  readonly id?: string;
  readonly payload?: unknown;
}

export interface SubscriptionHandlers {
  onNext(data: unknown): void;
  onError(error: MidnightRuntimeError): void;
  onComplete(): void;
}

function parseMessage(data: string | ArrayBuffer): ServerMessage {
  const text = typeof data === "string" ? data : new TextDecoder().decode(data);
  const parsed: unknown = JSON.parse(text);
  if (
    typeof parsed !== "object" ||
    parsed === null ||
    typeof (parsed as { type?: unknown }).type !== "string"
  ) {
    throw new MidnightRuntimeError("TRANSPORT_ERROR");
  }
  return parsed as ServerMessage;
}

export interface GraphQLSubscription {
  close(): void;
}

/** Routes one server frame. "ping"/"pong" and unknown frames are ignored. */
function dispatch(
  message: ServerMessage,
  handlers: SubscriptionHandlers,
  onAck: () => void,
): void {
  if (message.type === "connection_ack") {
    onAck();
    return;
  }
  if (message.type === "next") {
    const payload = message.payload;
    const data: unknown =
      typeof payload === "object" && payload !== null
        ? (payload as { data?: unknown }).data
        : undefined;
    handlers.onNext(data);
    return;
  }
  if (message.type === "error") {
    handlers.onError(new MidnightRuntimeError("TRANSPORT_ERROR"));
    return;
  }
  if (message.type === "complete") handlers.onComplete();
}

/**
 * Opens one subscription. Resolves once the server acknowledges the connection
 * and the subscribe frame has been sent; data arrives through the handlers.
 */
export function subscribe(
  url: string,
  query: string,
  variables: Readonly<Record<string, unknown>>,
  handlers: SubscriptionHandlers,
): Promise<GraphQLSubscription> {
  return new Promise<GraphQLSubscription>((resolve, reject) => {
    const socket = createReactNativeWebSocketFactory(GRAPHQL_WS_SUBPROTOCOL)(
      url,
    );
    const operationId = "1";
    let settled = false;
    let acknowledged = false;

    const handle: GraphQLSubscription = {
      close() {
        try {
          socket.send(JSON.stringify({ id: operationId, type: "complete" }));
        } catch {
          // The socket may already be gone; closing is best effort.
        }
        socket.close(1000, "cancelled");
      },
    };

    const fail = (error: MidnightRuntimeError) => {
      if (settled) {
        handlers.onError(error);
        return;
      }
      settled = true;
      socket.close(1000, "cancelled");
      reject(error);
    };

    socket.addEventListener("open", () => {
      socket.send(JSON.stringify({ type: "connection_init", payload: {} }));
    });

    const onAck = () => {
      if (acknowledged) return;
      acknowledged = true;
      socket.send(
        JSON.stringify({
          id: operationId,
          type: "subscribe",
          payload: { query, variables },
        }),
      );
      settled = true;
      resolve(handle);
    };

    socket.addEventListener("message", (event) => {
      let message: ServerMessage;
      try {
        message = parseMessage(event.data);
      } catch {
        fail(new MidnightRuntimeError("TRANSPORT_ERROR"));
        return;
      }
      dispatch(message, handlers, onAck);
    });

    socket.addEventListener("error", () => {
      fail(new MidnightRuntimeError("TRANSPORT_ERROR"));
    });
    socket.addEventListener("close", () => {
      if (!settled) {
        fail(new MidnightRuntimeError("TRANSPORT_ERROR"));
        return;
      }
      handlers.onComplete();
    });
  });
}
