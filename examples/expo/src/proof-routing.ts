import {
  MidnightRuntimeError,
  type MidnightFetch,
  type MidnightFetchRequest,
} from "@1am/midnight-mobile";

/**
 * Routes proof-server requests to the correct path.
 *
 * The runtime emits two different proof effects — `check` and `prove` — and a
 * real Midnight proof server serves them on two different paths. The transport
 * resolves an endpoint from the role alone (`endpoint()` maps role "proof" to
 * `proofServerUrl` verbatim, ignoring `step.effect`), so without this wrapper
 * both effects post to one URL and at most one of them can ever be right.
 *
 * The bodies are self-describing, which makes the routing exact rather than a
 * guess: each is a tagged serialization whose header names its own shape, and
 * the payload builders line up one-to-one with the server's paths.
 *
 *   create_check_payload   → (preimage, option(wrapped-ir))            → /check
 *   create_proving_payload → (preimage, option(proving-data),
 *                            option(fr-bls))                           → /prove
 *
 * Verified against proof-server 8.1.0: each path rejects the other's tag at the
 * header with `expected header tag …, got …`, and accepts its own before failing
 * deeper on the body. The tags below are the exact strings the server reports.
 *
 * `/prove-tx` also exists, taking a whole transaction plus a proving-data map.
 * The runtime never produces that shape — it proves preimage by preimage — so it
 * is deliberately not routed here.
 */

export const CHECK_TAG =
  "midnight:(proof-preimage-versioned,option(wrapped-ir)):";
export const PROVE_TAG =
  "midnight:(proof-preimage-versioned,option(proving-data),option(fr-bls)):";

const ROUTES: readonly (readonly [string, string])[] = [
  [CHECK_TAG, "check"],
  [PROVE_TAG, "prove"],
];

function startsWithAscii(body: Uint8Array, prefix: string): boolean {
  if (body.length < prefix.length) return false;
  for (let index = 0; index < prefix.length; index += 1) {
    if (body[index] !== prefix.charCodeAt(index)) return false;
  }
  return true;
}

/** Resolves the proof-server path a payload belongs to, by its own tag header. */
export function proofRouteFor(body: Uint8Array): string | undefined {
  return ROUTES.find(([tag]) => startsWithAscii(body, tag))?.[1];
}

function normalize(url: string): URL {
  try {
    return new URL(url);
  } catch {
    throw new MidnightRuntimeError("INVALID_ARGUMENT");
  }
}

/** True when a URL carries no path of its own, so appending one is safe. */
function isBareOrigin(url: URL): boolean {
  return url.pathname === "/" || url.pathname === "";
}

function routed(base: URL, route: string): string {
  const target = new URL(base.toString());
  target.pathname = `/${route}`;
  return target.toString();
}

/**
 * Wraps a transport `fetch` so proof requests reach the path their payload
 * belongs to. Everything else passes through untouched.
 *
 * Only a proof-server URL with no path of its own is rewritten. If an adopter
 * configures a URL that already carries a path, they have taken over routing —
 * silently replacing it would be worse than leaving it alone.
 *
 * An unrecognized body is also passed through unchanged rather than guessed at.
 * A wrong path fails at the server's header check with a clear message, which is
 * a far better failure than a payload silently proved as the wrong shape.
 */
export function createProofAwareFetch(
  inner: MidnightFetch,
  proofServerUrl: string,
): MidnightFetch {
  const base = normalize(proofServerUrl);
  const target = base.toString();
  if (!isBareOrigin(base)) return inner;
  return (url: string, request: MidnightFetchRequest) => {
    if (normalize(url).toString() !== target) return inner(url, request);
    const route = proofRouteFor(request.body);
    return route === undefined
      ? inner(url, request)
      : inner(routed(base, route), request);
  };
}
