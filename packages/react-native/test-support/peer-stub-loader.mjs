const reactSource = `
const cleanups = [];
export function createContext(value) {
  const context = { current: value, Provider: {} };
  context.Provider.context = context;
  return context;
}
export function createElement(type, props, children) {
  if (type?.context) type.context.current = props.value;
  return { type, props, children };
}
export function useContext(context) { return context.current; }
export function useEffect(effect) {
  const cleanup = effect();
  if (cleanup) cleanups.push(cleanup);
}
export function useMemo(factory) { return factory(); }
export function useState(initial) {
  let value = initial;
  return [value, (next) => { value = next; }];
}
export function __runEffectCleanups() {
  for (const cleanup of cleanups.splice(0)) cleanup();
}
`;

const reactNativeSource = `
let listener;
export const AppState = {
  addEventListener(_event, next) {
    listener = next;
    return { remove() { listener = undefined; } };
  },
  __emit(state) { listener?.(state); },
};
`;

const expoSource = `
export function requireNativeModule() {
  throw new Error("native module unavailable in tests");
}
`;

const sources = new Map([
  ["expo-modules-core", expoSource],
  ["react", reactSource],
  ["react-native", reactNativeSource],
]);

export function resolve(specifier, context, nextResolve) {
  return sources.has(specifier)
    ? { shortCircuit: true, url: `midnight-test:${specifier}` }
    : nextResolve(specifier, context);
}

export function load(url, context, nextLoad) {
  if (!url.startsWith("midnight-test:")) return nextLoad(url, context);
  const specifier = url.slice("midnight-test:".length);
  return {
    format: "module",
    shortCircuit: true,
    source: sources.get(specifier),
  };
}
