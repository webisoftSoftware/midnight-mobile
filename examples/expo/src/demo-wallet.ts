import type {
  MidnightWalletSessionConfig,
  MidnightWalletSessionSecrets,
} from "@1am/midnight-mobile";

/**
 * Public deterministic fixture for this example. These bytes are synthetic,
 * are not derived from a mnemonic, and must never be funded or reused.
 *
 * The SDK intentionally does not own mnemonic derivation. A real application
 * should pass keys and an address obtained from its wallet/key-management
 * layer, then clear the original key buffers after opening the session.
 */
export const DEMO_WALLET = {
  networkId: "preview",
  walletFingerprint: "public-example-wallet-v1",
  unshieldedAddress:
    "mn_addr_preview1xnd9uwtfx4mecjyrtflkpz6kwy4m4v90jspddjpqvqef2v46de9sk7ulc4",
} as const satisfies MidnightWalletSessionConfig;

/** A second public address derived from the same deterministic fixture. */
export const DEMO_RECIPIENT_ADDRESS =
  "mn_shield-addr_preview1k5f2raelljee2kzz5crzwtg70veyqh4l2w33ycjytay5z9eaw5l2ftj36j6jhegppwa4y3pdpa87lv343rt9ez5pxkfr0q7fp2zw22qxf08s0";

const DEMO_KEY_BYTES = {
  nightExternalKey: 0x11,
  zswapSeed: 0x22,
  dustSeed: 0x33,
} as const;

/** Returns fresh key arrays on every call so callers can safely wipe them. */
export function createDemoWalletFixture(): {
  readonly wallet: MidnightWalletSessionConfig;
  readonly secrets: MidnightWalletSessionSecrets;
} {
  return {
    wallet: { ...DEMO_WALLET },
    secrets: {
      nightExternalKey: new Uint8Array(32).fill(
        DEMO_KEY_BYTES.nightExternalKey,
      ),
      zswapSeed: new Uint8Array(32).fill(DEMO_KEY_BYTES.zswapSeed),
      dustSeed: new Uint8Array(32).fill(DEMO_KEY_BYTES.dustSeed),
    },
  };
}
