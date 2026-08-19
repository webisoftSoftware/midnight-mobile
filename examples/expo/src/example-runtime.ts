import {
  createMidnightRuntimeApi,
  createStandardMidnightTransport,
  InMemoryMidnightCheckpointStore,
  MidnightRuntimeController,
  type MidnightCheckpointStore,
  type MidnightFetch,
  type MidnightLogger,
  type MidnightRuntimeApi,
  type MidnightSessionHandle,
  type MidnightStandardTransport,
  type MidnightWalletSessionConfig,
  type MidnightWalletSessionSecrets,
  type MidnightWebSocketFactory,
} from "@1am/midnight-mobile";
import {
  createLocalProverMidnightTransport,
  type MidnightLocalProver,
} from "@1am/midnight-mobile/local-prover";

import { MockNativeRuntimeModule } from "./mock-native";
import {
  MOCK_NETWORK,
  MockMidnightServices,
  type ExamplePlatform,
} from "./mock-services";

export interface MockExampleRuntime {
  readonly platform: ExamplePlatform;
  readonly api: MidnightRuntimeApi;
  readonly transport: MidnightStandardTransport;
  readonly checkpointStore: InMemoryMidnightCheckpointStore;
  readonly services: MockMidnightServices;
  createController(): MidnightRuntimeController;
  createRestoredController(): MidnightRuntimeController;
}

function controller(
  api: MidnightRuntimeApi,
  transport: MidnightStandardTransport,
  checkpointStore: MidnightCheckpointStore,
): MidnightRuntimeController {
  return new MidnightRuntimeController({ api, transport, checkpointStore });
}

function createMockApi(): MidnightRuntimeApi {
  const native = new MockNativeRuntimeModule();
  return createMidnightRuntimeApi(() => native);
}

export function createMockExampleRuntime(
  platform: ExamplePlatform,
): MockExampleRuntime {
  const services = new MockMidnightServices();
  const checkpointStore = new InMemoryMidnightCheckpointStore();
  const transport = createStandardMidnightTransport({
    network: MOCK_NETWORK,
    fetch: services.fetch,
    createWebSocket: () => services.createWebSocket(),
    timeoutMs: 2_000,
    maximumEffectSteps: 8,
  });
  const api = createMockApi();
  return {
    platform,
    api,
    transport,
    checkpointStore,
    services,
    createController: () => controller(api, transport, checkpointStore),
    createRestoredController: () =>
      controller(createMockApi(), transport, checkpointStore),
  };
}

export interface LivePreviewRuntimeConfiguration {
  readonly mode: "live";
  readonly wallet: MidnightWalletSessionConfig & {
    readonly networkId: "preview";
  };
  readonly secrets: MidnightWalletSessionSecrets;
  readonly indexerHttpUrl: string;
  readonly indexerWebSocketUrl: string;
  readonly localProver: MidnightLocalProver;
  readonly nodeUrl: string;
  readonly fetch: MidnightFetch;
  readonly createWebSocket: MidnightWebSocketFactory;
  readonly endpointHeaders: (
    role: "indexer" | "proof" | "node",
  ) => Promise<Readonly<Record<string, string>>>;
  readonly checkpointStore?: MidnightCheckpointStore;
  readonly logger?: MidnightLogger;
}

export interface LivePreviewRuntime {
  readonly controller: MidnightRuntimeController;
  openWalletSession(): Promise<MidnightSessionHandle>;
}

function wipeSecrets(secrets: MidnightWalletSessionSecrets): void {
  secrets.nightExternalKey.fill(0);
  secrets.zswapSeed.fill(0);
  secrets.dustSeed.fill(0);
}

export function createLivePreviewRuntime(
  configuration: LivePreviewRuntimeConfiguration,
): LivePreviewRuntime {
  const transport = createLocalProverMidnightTransport(
    {
      network: {
        indexerHttpUrl: configuration.indexerHttpUrl,
        indexerWebSocketUrl: configuration.indexerWebSocketUrl,
        // Check and prove are handled by the native local prover. Balance-service
        // effects still use this placeholder and are intentionally out of scope
        // for this example's live session helper.
        proofServerUrl: "https://local-prover.invalid",
        nodeUrl: configuration.nodeUrl,
      },
      fetch: configuration.fetch,
      createWebSocket: configuration.createWebSocket,
      headers: configuration.endpointHeaders,
      ...(configuration.logger === undefined
        ? {}
        : { logger: configuration.logger }),
    },
    configuration.localProver,
  );
  const runtimeController = new MidnightRuntimeController({
    api: createMidnightRuntimeApi(),
    transport,
    ...(configuration.checkpointStore === undefined
      ? {}
      : { checkpointStore: configuration.checkpointStore }),
    ...(configuration.logger === undefined
      ? {}
      : { logger: configuration.logger }),
  });
  return {
    controller: runtimeController,
    async openWalletSession() {
      try {
        return await runtimeController.openWalletSession(
          configuration.wallet,
          configuration.secrets,
        );
      } finally {
        wipeSecrets(configuration.secrets);
      }
    },
  };
}
