import {
  createMidnightRuntimeApi,
  createStandardMidnightTransport,
  InMemoryMidnightCheckpointStore,
  MidnightRuntimeController,
  MidnightRuntimeError,
  type MidnightNetworkConfiguration,
  type MidnightSessionHandle,
  type MidnightWalletSessionConfig,
  type MidnightWalletSessionSecrets,
} from "@1am/midnight-mobile";

import {
  createReactNativeWebSocketFactory,
  GRAPHQL_WS_SUBPROTOCOL,
  reactNativeMidnightFetch,
} from "./live-services";
import type { NativeSmokeStep } from "./native-smoke";
import { callNodeMethod, createNodeAwareFetch } from "./node-submission";
import { createProofAwareFetch, PROVE_TAG } from "./proof-routing";
import { syncStream } from "./sync-driver";

/**
 * Live preview-network probe.
 *
 * This is deliberately incremental. It establishes, in order:
 *
 *   1. whether the device can reach each configured endpoint at all;
 *   2. whether the node answers as a real JSON-RPC node;
 *   3. whether the native runtime produces a sync request; and
 *   4. whether indexer payloads survive `applySyncBatch`, which is the only
 *      check that proves the wire mapping is actually correct.
 *
 * Step 4 covers the shielded and dust streams only. Those are keyless: the
 * indexer serves them by event id, and the device does its own trial
 * decryption. The unshielded stream is subscribed by address, so it needs a
 * real bech32m wallet address rather than the synthetic one below.
 *
 * Use a disposable wallet only. These seeds are synthetic and hold no funds.
 */

const SESSION: MidnightWalletSessionConfig = {
  networkId: "preview",
  walletFingerprint: "live-preview-probe",
  unshieldedAddress: "mn_shield-addr_undeployed1synthetic0live0probe",
};

const PROBE_TIMEOUT_MS = 15_000;

// The public preview indexer is a GraphQL server and rejects the handshake
// outright without this subprotocol. A private binary indexer would use the
// plain factory instead.
function createSyncSocketFactory() {
  return createReactNativeWebSocketFactory(GRAPHQL_WS_SUBPROTOCOL);
}

function syntheticSecrets(): MidnightWalletSessionSecrets {
  return {
    nightExternalKey: new Uint8Array(32).fill(11),
    zswapSeed: new Uint8Array(32).fill(12),
    dustSeed: new Uint8Array(32).fill(13),
  };
}

function wipe(secrets: MidnightWalletSessionSecrets): void {
  secrets.nightExternalKey.fill(0);
  secrets.zswapSeed.fill(0);
  secrets.dustSeed.fill(0);
}

function describe(failure: unknown): string {
  if (failure instanceof MidnightRuntimeError) return failure.code;
  if (failure instanceof Error) return failure.message;
  return String(failure);
}

/**
 * Reports reachability without naming the URL, so a tenant-identifying endpoint
 * never reaches the screen or a screenshot.
 */
async function probeEndpoint(
  label: string,
  url: string,
  expected: Readonly<Record<number, string>>,
  steps: NativeSmokeStep[],
): Promise<void> {
  const aborter = new AbortController();
  const timer = setTimeout(() => {
    aborter.abort();
  }, PROBE_TIMEOUT_MS);
  try {
    const response = await reactNativeMidnightFetch(url, {
      method: "POST",
      headers: { "content-type": "application/octet-stream" },
      body: new Uint8Array(0),
      signal: aborter.signal,
    });
    // Any HTTP status proves the device reached a server, so this step passes on
    // all of them. A 4xx is the normal answer: an empty octet-stream body is not
    // a valid request for any of these services. The known codes are explained
    // rather than shown bare, because three unexplained 4xx codes on screen read
    // as failures when they are in fact the expected result.
    const reason = expected[response.status];
    steps.push({
      name: `reach ${label}`,
      ok: true,
      detail:
        reason === undefined
          ? `HTTP ${String(response.status)} — reached; any status proves that`
          : `HTTP ${String(response.status)} as expected — ${reason}`,
    });
  } catch (failure: unknown) {
    steps.push({
      name: `reach ${label}`,
      ok: false,
      detail: describe(failure),
    });
  } finally {
    clearTimeout(timer);
  }
}

/**
 * Asks the node to identify itself over JSON-RPC.
 *
 * Unlike an empty POST, this distinguishes "a socket answered" from "a Midnight
 * node answered", which is the thing submission actually depends on.
 */
async function probeNodeRpc(
  network: MidnightNetworkConfiguration,
  steps: NativeSmokeStep[],
): Promise<void> {
  const aborter = new AbortController();
  const timer = setTimeout(() => {
    aborter.abort();
  }, PROBE_TIMEOUT_MS);
  try {
    const chain = await callNodeMethod(
      reactNativeMidnightFetch,
      network.nodeUrl,
      "system_chain",
      aborter.signal,
    );
    const version = await callNodeMethod(
      reactNativeMidnightFetch,
      network.nodeUrl,
      "midnight_ledgerVersion",
      aborter.signal,
    );
    steps.push({
      name: "node JSON-RPC",
      ok: true,
      detail: `${chain}, ledger ${version}`,
    });
  } catch (failure: unknown) {
    steps.push({
      name: "node JSON-RPC",
      ok: false,
      detail: describe(failure),
    });
  } finally {
    clearTimeout(timer);
  }
}

/**
 * Checks that the proof server expects the payload shape this runtime builds.
 *
 * An empty POST to /prove is refused with the tag header the server wants, which
 * makes this a real compatibility check rather than a ping: a proof server built
 * against a different ledger would name a different tag, and we find that out
 * here instead of part way through building a transaction.
 */
async function probeProofServer(
  network: MidnightNetworkConfiguration,
  steps: NativeSmokeStep[],
): Promise<void> {
  const aborter = new AbortController();
  const timer = setTimeout(() => {
    aborter.abort();
  }, PROBE_TIMEOUT_MS);
  try {
    const response = await reactNativeMidnightFetch(
      new URL("/prove", network.proofServerUrl).toString(),
      {
        method: "POST",
        headers: { "content-type": "application/octet-stream" },
        body: new Uint8Array(0),
        signal: aborter.signal,
      },
    );
    const reported = new TextDecoder().decode(
      new Uint8Array(await response.arrayBuffer()),
    );
    const matches = reported.includes(PROVE_TAG);
    steps.push({
      name: "proof server payload shape",
      ok: matches,
      detail: matches
        ? "expects the tag this runtime builds"
        : `server expects a different shape: ${reported.slice(0, 120)}`,
    });
  } catch (failure: unknown) {
    steps.push({
      name: "proof server payload shape",
      ok: false,
      detail: describe(failure),
    });
  } finally {
    clearTimeout(timer);
  }
}

async function probeReachability(
  network: MidnightNetworkConfiguration,
  steps: NativeSmokeStep[],
): Promise<void> {
  await probeEndpoint(
    "indexer",
    network.indexerHttpUrl,
    { 400: "an empty POST is not a GraphQL query" },
    steps,
  );
  await probeEndpoint(
    "proof server",
    network.proofServerUrl,
    { 404: "payloads go to /check and /prove, not the base path" },
    steps,
  );
  await probeEndpoint(
    "node",
    network.nodeUrl,
    // 415 is a useful signal rather than a nuisance: it is the node confirming
    // it will not take octet-stream, which is exactly why submissions are
    // translated to JSON-RPC.
    { 415: "the node speaks JSON-RPC only, as the submission adapter assumes" },
    steps,
  );
  await probeNodeRpc(network, steps);
  await probeProofServer(network, steps);
}

// Small on purpose. This probe is establishing that the mapping is correct, not
// syncing a wallet, and a short idle window keeps a quiet stream from stalling
// the screen.
const SYNC_LIMIT = 16;
const SYNC_IDLE_MS = 5_000;

/**
 * Syncs one stream end to end: runtime request, indexer subscription, and
 * `applySyncBatch`.
 *
 * The apply step is the whole point. Every payload struct on the Rust side is
 * `#[serde(deny_unknown_fields)]`, so a single extra field carried through from
 * GraphQL fails the batch with INVALID_ARGUMENT and no indication of which
 * field. Counting received payloads would look like success while proving
 * nothing; only a completed apply shows the projection is right.
 */
async function probeStream(
  controller: MidnightRuntimeController,
  session: MidnightSessionHandle,
  network: MidnightNetworkConfiguration,
  stream: "shielded" | "dust",
  steps: NativeSmokeStep[],
): Promise<void> {
  try {
    const progress = await syncStream(controller, session, stream, {
      indexerWebSocketUrl: network.indexerWebSocketUrl,
      unshieldedAddress: SESSION.unshieldedAddress,
      maxEvents: SYNC_LIMIT,
      idleMs: SYNC_IDLE_MS,
    });
    steps.push({
      name: `sync ${stream}`,
      ok: true,
      detail: `applied ${String(progress.applied)} events, offset ${String(progress.offset)}${progress.caughtUp ? ", caught up" : ""}`,
    });
  } catch (failure: unknown) {
    steps.push({
      name: `sync ${stream}`,
      ok: false,
      detail: describe(failure),
    });
  }
}

async function probeSyncStreams(
  controller: MidnightRuntimeController,
  session: MidnightSessionHandle,
  network: MidnightNetworkConfiguration,
  steps: NativeSmokeStep[],
): Promise<void> {
  await probeStream(controller, session, network, "shielded", steps);
  await probeStream(controller, session, network, "dust", steps);
}

export async function runLiveProbe(
  network: MidnightNetworkConfiguration,
): Promise<{
  readonly steps: readonly NativeSmokeStep[];
  readonly passed: boolean;
}> {
  const steps: NativeSmokeStep[] = [];
  await probeReachability(network, steps);

  const controller = new MidnightRuntimeController({
    api: createMidnightRuntimeApi(),
    transport: createStandardMidnightTransport({
      network,
      // Two host-layer adapters the transport cannot supply itself: submission
      // effects become author_submitExtrinsic JSON-RPC, and proof effects are
      // routed to /check or /prove by the tag their payload carries.
      fetch: createNodeAwareFetch(
        createProofAwareFetch(reactNativeMidnightFetch, network.proofServerUrl),
        network.nodeUrl,
      ),
      createWebSocket: createSyncSocketFactory(),
      timeoutMs: PROBE_TIMEOUT_MS,
    }),
    checkpointStore: new InMemoryMidnightCheckpointStore(),
  });
  const secrets = syntheticSecrets();
  let session: MidnightSessionHandle | null = null;
  try {
    session = await controller.openWalletSession(SESSION, secrets);
    steps.push({
      name: "open live wallet session",
      ok: true,
      detail: `session ${String(session.id)}`,
    });
  } catch (failure: unknown) {
    steps.push({
      name: "open live wallet session",
      ok: false,
      detail: describe(failure),
    });
  } finally {
    wipe(secrets);
  }

  if (session !== null) {
    await probeSyncStreams(controller, session, network, steps);
    try {
      await controller.closeWalletSession(session);
    } catch {
      // Reported failures above are the useful signal; a close error is not.
    }
  }
  await controller.dispose();
  return { steps, passed: steps.every((step) => step.ok) };
}
