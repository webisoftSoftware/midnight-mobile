import type { MidnightNetworkConfiguration } from "@1am/midnight-mobile";

/**
 * Live endpoint configuration, supplied entirely by the person running the test.
 *
 * No URL is committed here. The accepted scope forbids shipping operated-service
 * defaults, and that applies to the example too: a hardcoded indexer would be a
 * default in practice even if it only pointed at a public testnet. Values come
 * from `EXPO_PUBLIC_*` variables, which Expo inlines at bundle time.
 *
 * Set them before starting the bundler, for example in `.env.local`:
 *
 *   EXPO_PUBLIC_MIDNIGHT_INDEXER_HTTP=https://…
 *   EXPO_PUBLIC_MIDNIGHT_INDEXER_WS=wss://…
 *   EXPO_PUBLIC_MIDNIGHT_PROOF_SERVER=http://localhost:6300
 *   EXPO_PUBLIC_MIDNIGHT_NODE=https://…
 *
 * A local proof server is not reachable from a device by default. Forward it
 * with `adb reverse tcp:6300 tcp:6300`. Cleartext `http` is already permitted in
 * debug builds by `android/app/src/debug/AndroidManifest.xml`, so no manifest
 * change is needed for local testing; release builds do block it.
 */

interface FieldSpec {
  readonly field: keyof MidnightNetworkConfiguration;
  readonly variable: string;
  readonly schemes: readonly string[];
}

const FIELDS: readonly FieldSpec[] = [
  {
    field: "indexerHttpUrl",
    variable: "EXPO_PUBLIC_MIDNIGHT_INDEXER_HTTP",
    schemes: ["http:", "https:"],
  },
  {
    field: "indexerWebSocketUrl",
    variable: "EXPO_PUBLIC_MIDNIGHT_INDEXER_WS",
    schemes: ["ws:", "wss:"],
  },
  {
    field: "proofServerUrl",
    variable: "EXPO_PUBLIC_MIDNIGHT_PROOF_SERVER",
    schemes: ["http:", "https:"],
  },
  {
    field: "nodeUrl",
    variable: "EXPO_PUBLIC_MIDNIGHT_NODE",
    schemes: ["http:", "https:"],
  },
];

export type LiveConfigResult =
  | { readonly ok: true; readonly network: MidnightNetworkConfiguration }
  | { readonly ok: false; readonly problems: readonly string[] };

/** Narrows an untyped environment value at the boundary. */
function envValue(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined;
  const trimmed = value.trim();
  return trimmed.length === 0 ? undefined : trimmed;
}

function readVariable(name: string): string | undefined {
  // Expo replaces process.env.EXPO_PUBLIC_* with literals at build time, so
  // these must be read as static property accesses rather than dynamically.
  switch (name) {
    case "EXPO_PUBLIC_MIDNIGHT_INDEXER_HTTP":
      return envValue(process.env.EXPO_PUBLIC_MIDNIGHT_INDEXER_HTTP);
    case "EXPO_PUBLIC_MIDNIGHT_INDEXER_WS":
      return envValue(process.env.EXPO_PUBLIC_MIDNIGHT_INDEXER_WS);
    case "EXPO_PUBLIC_MIDNIGHT_PROOF_SERVER":
      return envValue(process.env.EXPO_PUBLIC_MIDNIGHT_PROOF_SERVER);
    case "EXPO_PUBLIC_MIDNIGHT_NODE":
      return envValue(process.env.EXPO_PUBLIC_MIDNIGHT_NODE);
    default:
      return undefined;
  }
}

function checkScheme(
  spec: FieldSpec,
  value: string,
  problems: string[],
): boolean {
  let parsed: URL;
  try {
    parsed = new URL(value);
  } catch {
    problems.push(`${spec.variable} is not a valid URL`);
    return false;
  }
  if (!spec.schemes.includes(parsed.protocol)) {
    problems.push(
      `${spec.variable} must use ${spec.schemes.join(" or ")}, got ${parsed.protocol}`,
    );
    return false;
  }
  return true;
}

export function readLiveConfig(): LiveConfigResult {
  const problems: string[] = [];
  const resolved: Partial<Record<keyof MidnightNetworkConfiguration, string>> =
    {};
  for (const spec of FIELDS) {
    const value = readVariable(spec.variable);
    if (value === undefined) {
      problems.push(`${spec.variable} is not set`);
      continue;
    }
    if (checkScheme(spec, value, problems)) resolved[spec.field] = value;
  }
  if (problems.length > 0) return { ok: false, problems };
  return {
    ok: true,
    network: {
      indexerHttpUrl: resolved.indexerHttpUrl ?? "",
      indexerWebSocketUrl: resolved.indexerWebSocketUrl ?? "",
      proofServerUrl: resolved.proofServerUrl ?? "",
      nodeUrl: resolved.nodeUrl ?? "",
    },
  };
}

/**
 * Reports which endpoints use cleartext.
 *
 * This is advisory, not a failure: a local proof server over `http` is the
 * expected setup for device testing. It matters because release builds block
 * cleartext, so anything listed here would stop working outside a debug build.
 */
export function cleartextEndpoints(
  network: MidnightNetworkConfiguration,
): readonly string[] {
  // Enumerate the known fields rather than using Object.entries, which widens
  // the value type and would push an `any` through this boundary.
  return FIELDS.filter((spec) => /^(http|ws):/u.test(network[spec.field])).map(
    (spec) => spec.field,
  );
}
