const reactSource = `
export function createContext(value) {
  return { current: value, Provider: { context: null } };
}
export function createElement() { return null; }
export function useContext(context) { return context.current; }
export function useEffect() {}
export function useMemo(factory) { return factory(); }
export function useState(initial) { return [initial, () => undefined]; }
`;

const reactNativeSource = `
export const AppState = {
  addEventListener() { return { remove() {} }; },
};
`;

const expoModulesSource = `
export function requireNativeModule() {
  throw new Error("native module unavailable in mock lifecycle tests");
}
`;

const sources = new Map([
  ["expo-modules-core", expoModulesSource],
  ["react", reactSource],
  ["react-native", reactNativeSource],
]);

export async function resolve(specifier, context, nextResolve) {
  if (sources.has(specifier)) {
    return { shortCircuit: true, url: `midnight-example-test:${specifier}` };
  }
  try {
    return await nextResolve(specifier, context);
  } catch (error) {
    const relativeWithoutExtension =
      specifier.startsWith(".") && !/\.[cm]?js$/.test(specifier);
    if (!relativeWithoutExtension) throw error;
    return nextResolve(`${specifier}.js`, context);
  }
}

export function load(url, context, nextLoad) {
  if (!url.startsWith("midnight-example-test:")) {
    return nextLoad(url, context);
  }
  const specifier = url.slice("midnight-example-test:".length);
  return {
    format: "module",
    shortCircuit: true,
    source: sources.get(specifier),
  };
}
