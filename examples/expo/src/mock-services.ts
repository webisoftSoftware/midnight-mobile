import {
  MidnightRuntimeError,
  type MidnightFetch,
  type MidnightFetchResponse,
  type MidnightWebSocket,
} from "@1am/midnight-mobile";

export type ExamplePlatform = "ios" | "android";

export const MOCK_NETWORK = {
  indexerHttpUrl: "https://indexer.mock.invalid/request",
  indexerWebSocketUrl: "wss://indexer.mock.invalid/stream",
  proofServerUrl: "https://proof.mock.invalid/request",
  nodeUrl: "https://node.mock.invalid/request",
} as const;

type ProofBehavior = "accept" | "fail";

function response(bytes: readonly number[]): MidnightFetchResponse {
  return {
    status: 200,
    arrayBuffer() {
      const buffer = new ArrayBuffer(bytes.length);
      new Uint8Array(buffer).set(bytes);
      return Promise.resolve(buffer);
    },
  };
}

export class MockMidnightServices {
  #nextProofBehavior: ProofBehavior = "accept";

  readonly fetch: MidnightFetch = (url, request) => {
    if (request.signal.aborted) {
      return Promise.reject(new MidnightRuntimeError("CANCELLED"));
    }
    if (url === MOCK_NETWORK.proofServerUrl) {
      return this.#proofResponse();
    }
    if (url !== MOCK_NETWORK.indexerHttpUrl && url !== MOCK_NETWORK.nodeUrl) {
      return Promise.reject(new MidnightRuntimeError("TRANSPORT_ERROR"));
    }
    return Promise.resolve(response([7, 8, 9]));
  };

  failNextProofRequest(): void {
    this.#nextProofBehavior = "fail";
  }

  createWebSocket(): MidnightWebSocket {
    return new MockMidnightWebSocket();
  }

  #proofResponse(): Promise<MidnightFetchResponse> {
    if (this.#nextProofBehavior === "fail") {
      this.#nextProofBehavior = "accept";
      return Promise.reject(new Error("synthetic proof service failure"));
    }
    return Promise.resolve(response([4, 5, 6]));
  }
}

class MockMidnightWebSocket implements MidnightWebSocket {
  readonly readyState = 1;

  send(data: string | Uint8Array): void {
    void data;
  }

  close(code?: number, reason?: string): void {
    void code;
    void reason;
  }

  addEventListener(
    type: "message",
    listener: (event: { readonly data: string | ArrayBuffer }) => void,
  ): void;
  addEventListener(
    type: "open" | "error" | "close",
    listener: () => void,
  ): void;
  addEventListener(
    type: "message" | "open" | "error" | "close",
    listener:
      (() => void) | ((event: { readonly data: string | ArrayBuffer }) => void),
  ): void {
    void type;
    void listener;
  }
}
