import {
  MidnightRuntimeError,
  type MidnightFetch,
  type MidnightFetchResponse,
  type MidnightWebSocket,
  type MidnightWebSocketFactory,
} from "@1am/midnight-mobile";

/**
 * React Native host implementations of the SDK's HTTP and WebSocket contracts.
 *
 * The package deliberately does not read global `fetch` or `WebSocket`, so an
 * adopter must supply both. These are the reference implementations for a live
 * network, and the mocks in `mock-services.ts` remain the default for tests.
 *
 * This uses `XMLHttpRequest` rather than `fetch` on purpose. React Native's
 * fetch routes binary bodies and responses through the Blob polyfill, where
 * `response.arrayBuffer()` is unreliable across platforms and RN versions. XHR
 * with `responseType = "arraybuffer"` sends and receives bytes without text
 * conversion, reports the original status, and maps abort cleanly.
 *
 * Per the network configuration contract, nothing here logs URLs, headers,
 * bodies, or credentials.
 */

function toResponse(request: XMLHttpRequest): MidnightFetchResponse {
  const body: unknown = request.response;
  return {
    status: request.status,
    arrayBuffer(): Promise<ArrayBuffer> {
      if (body instanceof ArrayBuffer) return Promise.resolve(body);
      // A proxy or error page can return a non-binary body even when
      // responseType asked for bytes. Surface that as a transport failure
      // rather than silently handing the runtime an empty buffer.
      return Promise.reject(new MidnightRuntimeError("TRANSPORT_ERROR"));
    },
  };
}

export const reactNativeMidnightFetch: MidnightFetch = (url, request) =>
  new Promise<MidnightFetchResponse>((resolve, reject) => {
    if (request.signal.aborted) {
      reject(new MidnightRuntimeError("CANCELLED"));
      return;
    }
    const xhr = new XMLHttpRequest();
    xhr.open(request.method, url, true);
    xhr.responseType = "arraybuffer";
    for (const [name, value] of Object.entries(request.headers)) {
      xhr.setRequestHeader(name, value);
    }

    const onAbort = () => {
      xhr.abort();
      reject(new MidnightRuntimeError("CANCELLED"));
    };
    request.signal.addEventListener("abort", onAbort, { once: true });
    const settle = () => {
      request.signal.removeEventListener("abort", onAbort);
    };

    xhr.onload = () => {
      settle();
      resolve(toResponse(xhr));
    };
    xhr.onerror = () => {
      settle();
      reject(new MidnightRuntimeError("TRANSPORT_ERROR"));
    };
    xhr.ontimeout = () => {
      settle();
      reject(new MidnightRuntimeError("TRANSPORT_ERROR"));
    };
    // The SDK owns timeout and cancellation through the abort signal, so the
    // transport must not impose a second, conflicting deadline here.
    xhr.timeout = 0;
    // Send a detached ArrayBuffer copy. A generic Uint8Array does not satisfy
    // BufferSource under current DOM types, and copying also means a caller that
    // wipes its buffer cannot race this request into sending zeroes.
    const body = new Uint8Array(new ArrayBuffer(request.body.byteLength));
    body.set(request.body);
    xhr.send(body.buffer);
  });

interface RawWebSocket {
  readyState: number;
  binaryType: string;
  send(data: string | Uint8Array): void;
  close(code?: number, reason?: string): void;
  addEventListener(type: string, listener: (event: unknown) => void): void;
}

type RawWebSocketConstructor = new (
  url: string,
  protocols?: string | readonly string[],
) => RawWebSocket;

function socketConstructor(): RawWebSocketConstructor {
  const candidate: unknown = globalThis.WebSocket;
  if (typeof candidate !== "function") {
    throw new MidnightRuntimeError("UNAVAILABLE");
  }
  return candidate as RawWebSocketConstructor;
}

function messageData(event: unknown): string | ArrayBuffer {
  const data: unknown = (event as { readonly data?: unknown }).data;
  if (typeof data === "string" || data instanceof ArrayBuffer) return data;
  throw new MidnightRuntimeError("TRANSPORT_ERROR");
}

/**
 * Builds a socket factory, optionally negotiating a WebSocket subprotocol.
 *
 * `MidnightWebSocketFactory` receives only a URL, so a subprotocol has to be
 * closed over here. That is not optional for a GraphQL indexer: verified against
 * the public Midnight preview indexer, the handshake returns 400 with no
 * subprotocol and 101 with `graphql-transport-ws`.
 */
export function createReactNativeWebSocketFactory(
  protocols?: string | readonly string[],
): MidnightWebSocketFactory {
  return (url: string): MidnightWebSocket => openSocket(url, protocols);
}

export const GRAPHQL_WS_SUBPROTOCOL = "graphql-transport-ws";

function openSocket(
  url: string,
  protocols?: string | readonly string[],
): MidnightWebSocket {
  const Socket = socketConstructor();
  const socket =
    protocols === undefined ? new Socket(url) : new Socket(url, protocols);
  // The indexer sync stream is binary. Without this React Native delivers
  // Blobs, which the SDK's socket contract does not accept.
  socket.binaryType = "arraybuffer";
  return {
    get readyState(): number {
      return socket.readyState;
    },
    send(data: string | Uint8Array): void {
      socket.send(data);
    },
    close(code?: number, reason?: string): void {
      socket.close(code, reason);
    },
    addEventListener(
      type: "message" | "open" | "error" | "close",
      listener: ((event: { readonly data: string | ArrayBuffer }) => void) &
        (() => void),
    ): void {
      if (type === "message") {
        socket.addEventListener("message", (event: unknown) => {
          listener({ data: messageData(event) });
        });
        return;
      }
      socket.addEventListener(type, () => {
        listener();
      });
    },
  };
}

/** Plain factory with no subprotocol, for a binary (non-GraphQL) indexer. */
export const reactNativeMidnightWebSocket: MidnightWebSocketFactory = (
  url: string,
): MidnightWebSocket => openSocket(url);
