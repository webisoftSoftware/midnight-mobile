import {
  createMidnightRuntimeApi,
  createStandardMidnightTransport,
  InMemoryMidnightCheckpointStore,
  MidnightRuntimeController,
  type MidnightCheckpointStore,
  type MidnightRuntimeApi,
  type MidnightStandardTransport,
} from "@1am/midnight-mobile";

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
    createController: () => controller(api, transport, checkpointStore),
    createRestoredController: () =>
      controller(createMockApi(), transport, checkpointStore),
  };
}
