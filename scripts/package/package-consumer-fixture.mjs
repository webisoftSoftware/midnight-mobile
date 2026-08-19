import { mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

function consumerSource() {
  return `import {
  InMemoryMidnightCheckpointStore,
  MidnightRuntimeController,
  MidnightRuntimeProvider,
  createMidnightRuntimeApi,
  createStandardMidnightTransport,
  useMidnightRuntime,
  type MidnightCheckpoint,
  type MidnightCommand,
  type MidnightCommandResult,
  type MidnightFetch,
  type MidnightLogger,
  type MidnightNetworkConfiguration,
  type MidnightRuntimeApi,
  type MidnightRuntimeContextValue,
  type MidnightRuntimeErrorCode,
  type MidnightRuntimeProviderProps,
  type MidnightSessionHandle,
  type MidnightWalletSnapshot,
} from "@1am/midnight-mobile";

export const publicValues = [
  InMemoryMidnightCheckpointStore,
  MidnightRuntimeController,
  MidnightRuntimeProvider,
  createMidnightRuntimeApi,
  createStandardMidnightTransport,
  useMidnightRuntime,
];
export type PublicTypes = readonly [
  MidnightCheckpoint,
  MidnightCommand<"transfer">,
  MidnightCommandResult<"transfer">,
  MidnightFetch,
  MidnightLogger,
  MidnightNetworkConfiguration,
  MidnightRuntimeApi,
  MidnightRuntimeContextValue,
  MidnightRuntimeErrorCode,
  MidnightRuntimeProviderProps,
  MidnightSessionHandle,
  MidnightWalletSnapshot,
];
`;
}

export function writeConsumerFiles(consumer, tarball) {
  mkdirSync(consumer, { recursive: true });
  writeFileSync(
    resolve(consumer, "package.json"),
    `${JSON.stringify(
      {
        name: "midnight-mobile-package-consumer",
        version: "0.0.0",
        private: true,
        type: "module",
        dependencies: {
          "@1am/midnight-mobile": pathToFileURL(tarball).href,
        },
      },
      null,
      2,
    )}\n`,
  );
  writeFileSync(resolve(consumer, "consumer.ts"), consumerSource());
  writeFileSync(
    resolve(consumer, "tsconfig.json"),
    `${JSON.stringify(
      {
        compilerOptions: {
          exactOptionalPropertyTypes: true,
          lib: ["ES2022", "DOM"],
          module: "NodeNext",
          moduleResolution: "NodeNext",
          noEmit: true,
          noUncheckedIndexedAccess: true,
          skipLibCheck: true,
          strict: true,
          target: "ES2022",
        },
        include: ["consumer.ts"],
      },
      null,
      2,
    )}\n`,
  );
}
