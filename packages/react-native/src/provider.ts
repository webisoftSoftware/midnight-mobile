import {
  createContext,
  createElement,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { AppState } from "react-native";

import {
  MidnightRuntimeController,
  type MidnightRuntimeControllerOptions,
} from "./controller.js";
import { normalizeMidnightError, type MidnightRuntimeError } from "./errors.js";
import type { MidnightRuntimeStatus } from "./runtime-types.js";

export interface MidnightRuntimeContextValue {
  readonly controller: MidnightRuntimeController;
  readonly status: MidnightRuntimeStatus;
  readonly error: MidnightRuntimeError | null;
}

const MidnightRuntimeContext =
  createContext<MidnightRuntimeContextValue | null>(null);

export interface MidnightRuntimeProviderProps extends MidnightRuntimeControllerOptions {
  readonly children?: ReactNode;
  readonly onError?: (error: MidnightRuntimeError) => void;
}

export function MidnightRuntimeProvider(
  props: MidnightRuntimeProviderProps,
): ReactNode {
  const { api, checkpointStore, children, logger, onError, transport } = props;
  const controller = useMemo(
    () =>
      new MidnightRuntimeController({
        api,
        transport,
        ...(checkpointStore === undefined ? {} : { checkpointStore }),
        ...(logger === undefined ? {} : { logger }),
      }),
    [api, checkpointStore, logger, transport],
  );
  const [status, setStatus] = useState<MidnightRuntimeStatus>(
    controller.status,
  );
  const [error, setError] = useState<MidnightRuntimeError | null>(null);

  useEffect(() => {
    const unsubscribe = controller.subscribe(setStatus);
    return () => {
      unsubscribe();
      return undefined;
    };
  }, [controller]);
  useEffect(() => {
    const report = (failure: unknown) => {
      const normalized = normalizeMidnightError(failure);
      setError(normalized);
      onError?.(normalized);
    };
    const subscription = AppState.addEventListener("change", (nextState) => {
      const action =
        nextState === "active" ? controller.refresh() : controller.pause();
      void action.catch(report);
    });
    return () => {
      subscription.remove();
      void controller.dispose().catch(report);
      return undefined;
    };
  }, [controller, onError]);

  const value = useMemo(
    () => ({ controller, status, error }),
    [controller, error, status],
  );
  return createElement(MidnightRuntimeContext.Provider, { value }, children);
}

export function useMidnightRuntime(): MidnightRuntimeContextValue {
  const value = useContext(MidnightRuntimeContext);
  if (value === null) {
    throw new Error(
      "useMidnightRuntime must be used within MidnightRuntimeProvider",
    );
  }
  return value;
}
