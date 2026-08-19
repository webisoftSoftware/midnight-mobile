import {
  createMidnightRuntimeApi,
  MidnightRuntimeError,
  type MidnightCheckpoint,
  type MidnightRuntimeApi,
  type MidnightSessionHandle,
  type MidnightWalletSessionSecrets,
} from "@1am/midnight-mobile";

import { createDemoWalletFixture } from "./demo-wallet";

/**
 * On-device smoke test for the prebuilt native runtime.
 *
 * Every step below is a pure Rust operation: no indexer, proof server, node, or
 * network access of any kind is involved. That is the point. The mocked
 * lifecycle in `lifecycle.ts` proves the TypeScript layer against a fake native
 * module; this proves the packaged `.so`/XCFramework actually loads on real
 * hardware and that the UniFFI bindings resolve, which nothing else in the
 * repository checks.
 *
 * Seeds here are deterministic synthetic bytes. They are not a wallet, hold no
 * funds, and must never be replaced with real key material.
 */

export interface NativeSmokeStep {
  readonly name: string;
  readonly ok: boolean;
  readonly detail: string;
}

export interface NativeSmokeReport {
  readonly steps: readonly NativeSmokeStep[];
  readonly passed: boolean;
}

type Outcome<T> =
  { readonly ok: true; readonly value: T } | { readonly ok: false };

function wipe(secrets: MidnightWalletSessionSecrets): void {
  secrets.nightExternalKey.fill(0);
  secrets.zswapSeed.fill(0);
  secrets.dustSeed.fill(0);
}

function describeFailure(failure: unknown): string {
  if (failure instanceof MidnightRuntimeError) return failure.code;
  if (failure instanceof Error) return failure.message;
  return String(failure);
}

/**
 * Checkpoints hold privacy-sensitive wallet state, so report only the byte
 * length and a short prefix. Never render the full contents.
 */
function describeCheckpoint(checkpoint: MidnightCheckpoint): string {
  const prefix = Array.from(checkpoint.bytes.slice(0, 4))
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
  const size = String(checkpoint.bytes.length);
  return `v${String(checkpoint.version)}, ${size} bytes, starts ${prefix}`;
}

/**
 * Runs one step, records its outcome, and returns the value so the caller keeps
 * ordinary type narrowing. Assigning to an outer variable from inside the
 * callback would defeat it.
 */
async function attempt<T>(
  steps: NativeSmokeStep[],
  name: string,
  run: () => Promise<readonly [T, string]> | readonly [T, string],
): Promise<Outcome<T>> {
  try {
    const [value, detail] = await run();
    steps.push({ name, ok: true, detail });
    return { ok: true, value };
  } catch (failure: unknown) {
    steps.push({ name, ok: false, detail: describeFailure(failure) });
    return { ok: false };
  }
}

async function closeQuietly(
  api: MidnightRuntimeApi,
  session: MidnightSessionHandle,
): Promise<void> {
  try {
    await api.closeWalletSession(session);
  } catch {
    // A close failure must not mask the step that already reported.
  }
}

async function signData(
  api: MidnightRuntimeApi,
  session: MidnightSessionHandle,
): Promise<readonly [null, string]> {
  const step = await api.beginCommand(session, {
    kind: "signData",
    domain: "midnight-mobile/device-smoke",
    dataBase64: "ZGV2aWNlLXNtb2tl",
  });
  if (step.kind !== "complete") {
    throw new Error(`signData did not complete natively: ${step.kind}`);
  }
  const bytes = String(step.result.signatureHex.length / 2);
  const key = step.result.verifyingKeyHex.slice(0, 8);
  return [null, `signature ${bytes} bytes, key ${key}`];
}

async function exerciseSession(
  api: MidnightRuntimeApi,
  steps: NativeSmokeStep[],
): Promise<MidnightCheckpoint | null> {
  const { wallet, secrets } = createDemoWalletFixture();
  const opened = await attempt(steps, "open wallet session", async () => {
    const session = await api.openWalletSession(wallet, secrets);
    const detail = `session ${String(session.id)} generation ${String(session.generation)}`;
    return [session, detail] as const;
  });
  wipe(secrets);
  if (!opened.ok) return null;
  const session = opened.value;

  try {
    await attempt(steps, "read wallet snapshot", async () => {
      const snapshot = await api.getWalletSnapshot(session);
      return [null, `network ${snapshot.networkId}`] as const;
    });

    const exported = await attempt(steps, "export checkpoint", async () => {
      const checkpoint = await api.exportWalletCheckpoint(session);
      return [checkpoint, describeCheckpoint(checkpoint)] as const;
    });

    await attempt(steps, "sign domain-separated data", () =>
      signData(api, session),
    );

    await attempt(steps, "close wallet session", async () => {
      await api.closeWalletSession(session);
      return [null, "closed"] as const;
    });

    return exported.ok ? exported.value : null;
  } catch (failure: unknown) {
    steps.push({
      name: "session teardown",
      ok: false,
      detail: describeFailure(failure),
    });
    await closeQuietly(api, session);
    return null;
  }
}

async function restoreCheckpoint(
  api: MidnightRuntimeApi,
  steps: NativeSmokeStep[],
  checkpoint: MidnightCheckpoint,
): Promise<void> {
  const { wallet, secrets } = createDemoWalletFixture();
  await attempt(steps, "restore from checkpoint", async () => {
    const restored = await api.openWalletSession(wallet, secrets, checkpoint);
    try {
      const snapshot = await api.getWalletSnapshot(restored);
      return [null, `restored on ${snapshot.networkId}`] as const;
    } finally {
      await closeQuietly(api, restored);
    }
  });
  wipe(secrets);
}

export async function runNativeSmokeTest(): Promise<NativeSmokeReport> {
  const steps: NativeSmokeStep[] = [];
  // No loader argument: this resolves the real prebuilt native module rather
  // than the mock. If the packaged library is missing, built for the wrong ABI,
  // or its UniFFI checksums disagree with the generated bindings, this is where
  // it fails.
  const loaded = await attempt(steps, "load native module", () => [
    createMidnightRuntimeApi(),
    "resolved prebuilt runtime",
  ]);
  if (loaded.ok) {
    const checkpoint = await exerciseSession(loaded.value, steps);
    if (checkpoint !== null) {
      await restoreCheckpoint(loaded.value, steps, checkpoint);
    }
  }
  return { steps, passed: steps.every((step) => step.ok) };
}
