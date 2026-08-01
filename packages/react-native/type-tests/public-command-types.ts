import type {
  MidnightCommand,
  MidnightCommandKind,
  MidnightCommandResult,
} from "../src/index.js";

type PublicCommandTypeFixtures = {
  readonly [K in MidnightCommandKind]: readonly [
    command: MidnightCommand<K>,
    result: MidnightCommandResult<K>,
  ];
};

const hash = "11".repeat(32);
const signature = "22".repeat(64);
const verifyingKey = "33".repeat(32);

export const PUBLIC_COMMAND_TYPE_FIXTURES = {
  signData: [
    { kind: "signData", domain: "example", dataBase64: "AA==" },
    { signatureHex: signature, verifyingKeyHex: verifyingKey },
  ],
  createCheckPayload: [
    { kind: "createCheckPayload", preimageBase64: "AA==", irBase64: "AQ==" },
    { payloadBase64: "Ag==" },
  ],
  parseCheckResult: [
    { kind: "parseCheckResult", resultBase64: "AA==" },
    { values: ["1", null] },
  ],
  createProvingPayload: [
    {
      kind: "createProvingPayload",
      preimageBase64: "AA==",
      bindingInput: "binding",
      keyMaterial: {
        proverKeyBase64: "AQ==",
        verifierKeyBase64: "Ag==",
        irBase64: "Aw==",
        compression: "gzip",
      },
    },
    { payloadBase64: "BA==" },
  ],
  canonicalizeTransaction: [
    {
      kind: "canonicalizeTransaction",
      signatureMarker: "signature",
      proofMarker: "proof",
      bindingMarker: "binding",
      rawBase64: "AA==",
    },
    { transactionBase64: "AQ==" },
  ],
  createSyncRequest: [
    {
      kind: "createSyncRequest",
      stream: "shielded",
      fromOffset: 0,
      limit: 100,
    },
    { stream: "shielded", fromOffset: 0, requestBase64: "AA==" },
  ],
  deriveShieldedMintContext: [
    { kind: "deriveShieldedMintContext" },
    {
      coinPublicKeyHex: hash,
      encryptionPublicKeyHex: hash,
      outputIndex: 0,
    },
  ],
  watchShieldedMint: [
    {
      kind: "watchShieldedMint",
      coinInfoBase64: "AA==",
      expectedOutputIndex: 0,
    },
    { outputIndex: 0 },
  ],
  createShieldedSpentRequest: [
    { kind: "createShieldedSpentRequest" },
    { requestBase64: "AA==", nullifierCount: 1 },
  ],
  applyShieldedSpentResponse: [
    { kind: "applyShieldedSpentResponse", resultBase64: "AA==" },
    { removedCount: 1 },
  ],
  setShieldedProtocolVersion: [
    {
      kind: "setShieldedProtocolVersion",
      protocolVersion: 1,
      syncOffset: 0,
    },
    { protocolVersion: 1 },
  ],
  createDustSpendRequest: [
    { kind: "createDustSpendRequest" },
    { requestBase64: "AA==", utxoCount: 1 },
  ],
  createDustCommitmentRequest: [
    {
      kind: "createDustCommitmentRequest",
      rawBase64: "AA==",
      syncOffset: 0,
    },
    { status: "rebuild", requestBase64: "AQ==" },
  ],
  applyDustSpendResolution: [
    {
      kind: "applyDustSpendResolution",
      rawBase64: "AA==",
      resultBase64: "AQ==",
      syncOffset: 0,
    },
    { status: "applied" },
  ],
  transfer: [
    {
      kind: "transfer",
      to: "mn_addr",
      amount: "1",
      tokenType: "night",
      walletType: "shielded",
    },
    {
      transactionBase64: "AA==",
      transactionHash: hash,
      ledgerTransactionHash: hash,
      identifiers: ["transfer"],
      expiresAt: 1,
    },
  ],
  dappTransfer: [
    {
      kind: "dappTransfer",
      outputs: [
        {
          walletType: "unshielded",
          tokenType: "night",
          amount: "1",
          receiverAddress: "mn_addr",
        },
      ],
    },
    {
      transactionBase64: "AA==",
      transactionHash: hash,
      ledgerTransactionHash: hash,
      identifiers: ["dapp-transfer"],
    },
  ],
  dappIntent: [
    {
      kind: "dappIntent",
      inputs: [{ walletType: "shielded", tokenType: "night", amount: "1" }],
      outputs: [
        {
          walletType: "shielded",
          tokenType: "night",
          amount: "1",
          receiverAddress: "mn_addr",
        },
      ],
    },
    {
      transactionBase64: "AA==",
      transactionHash: hash,
      ledgerTransactionHash: hash,
      identifiers: ["dapp-intent"],
    },
  ],
  generateDust: [
    {
      kind: "generateDust",
      ledgerParametersBase64: "AA==",
      feeBlocksMargin: 1,
      additionalFeeOverhead: "0",
    },
    {
      transactionBase64: "AA==",
      transactionHash: hash,
      ledgerTransactionHash: hash,
      identifiers: ["generate-dust"],
    },
  ],
  balanceUnsealed: [
    {
      kind: "balanceUnsealed",
      ledgerParametersBase64: "AA==",
      rawBase64: "AQ==",
      feeBlocksMargin: 1,
      additionalFeeOverhead: "0",
    },
    {
      transactionBase64: "AA==",
      transactionHash: hash,
      ledgerTransactionHash: hash,
      identifiers: ["balance-unsealed"],
    },
  ],
  balanceSealed: [
    {
      kind: "balanceSealed",
      ledgerParametersBase64: "AA==",
      rawBase64: "AQ==",
      feeBlocksMargin: 1,
      additionalFeeOverhead: "0",
    },
    {
      transactionBase64: "AA==",
      transactionHash: hash,
      ledgerTransactionHash: hash,
      identifiers: ["balance-sealed"],
    },
  ],
  finalizeUnprovenTransaction: [
    {
      kind: "finalizeUnprovenTransaction",
      rawBase64: "AA==",
      keyMaterial: {
        "midnight/zswap/spend": {
          proverKeyBase64: "AQ==",
          verifierKeyBase64: "Ag==",
          irBase64: "Aw==",
        },
      },
    },
    {
      transactionBase64: "AA==",
      transactionHash: hash,
      ledgerTransactionHash: hash,
      identifiers: ["finalized-unproven"],
    },
  ],
  submitFinalized: [
    { kind: "submitFinalized", rawBase64: "AA==" },
    {
      transactionHash: hash,
      identifiers: ["submit"],
      status: "accepted",
      bodyBase64: "AQ==",
    },
  ],
} as const satisfies PublicCommandTypeFixtures;
