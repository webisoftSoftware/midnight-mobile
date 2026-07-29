declare module "expo-modules-core" {
  export function requireNativeModule<T>(name: string, typeHint?: () => T): T;
}

declare module "react" {
  export type ReactNode = object | string | number | boolean | null | undefined;
  export interface Context<T> {
    readonly Provider: {
      readonly contextValue?: T;
    };
  }
  export function createContext<T>(value: T): Context<T>;
  export function createElement(
    type: object,
    props: object,
    children?: ReactNode,
  ): ReactNode;
  export function useContext<T>(context: Context<T>): T;
  export function useEffect(
    effect: () => undefined | (() => undefined),
    dependencies: readonly unknown[],
  ): void;
  export function useMemo<T>(
    factory: () => T,
    dependencies: readonly unknown[],
  ): T;
  export function useState<T>(initial: T): [T, (value: T) => void];
  export function __runEffectCleanups(): void;
}

declare module "react-native" {
  export const AppState: {
    addEventListener(
      event: "change",
      listener: (state: string) => void,
    ): { remove(): void };
    __emit(state: string): void;
  };
}
