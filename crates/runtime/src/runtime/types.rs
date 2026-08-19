use super::*;

pub(super) const CHECKPOINT_VERSION: u32 = 1;
pub(super) const CHECKPOINT_MAGIC: &[u8; 4] = b"MMCP";
pub(super) const MAX_OPEN_SESSIONS: usize = 2;
pub(super) const MAX_SYNC_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;
pub(super) const MAX_CHECKPOINT_BYTES: usize = 64 * 1024 * 1024;
pub(super) const MAX_SIGNING_DOMAIN_BYTES: usize = 1024;
pub(super) const MAX_DAPP_SIGN_DATA_BYTES: usize = 1024 * 1024;
pub(super) const MAX_SYNC_BATCH_RECEIPTS: usize = 4096;
pub(super) const MAX_CANCELLED_OPERATION_TOMBSTONES: usize = 1024;
pub(super) const RESERVED_OPERATION_ID: u64 = u64::MAX;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct WalletSessionConfig {
    pub(super) network_id: String,
    pub(super) wallet_fingerprint: String,
    pub(super) unshielded_address: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StreamOffset {
    pub(super) stream: String,
    pub(super) next_offset: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct BatchReceipt {
    pub(super) stream: String,
    pub(super) from_offset: u64,
    pub(super) to_offset: u64,
    pub(super) digest: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) enum SubmissionStatus {
    AwaitingResponse,
    Accepted,
    Rejected,
    StatusUnknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PendingSubmission {
    pub(super) transaction_hash: String,
    pub(super) identifiers: Vec<String>,
    pub(super) status: SubmissionStatus,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct WalletCheckpoint {
    pub(super) version: u32,
    pub(super) network_id: String,
    pub(super) wallet_fingerprint: String,
    pub(super) legacy_state: LegacyWalletState,
    pub(super) stream_offsets: Vec<StreamOffset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) caught_up_streams: Vec<String>,
    #[serde(default)]
    pub(super) batch_receipts: Vec<BatchReceipt>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) pending_submissions: Vec<PendingSubmission>,
    pub(super) ledger_revision: String,
    pub(super) generation: u64,
    pub(super) checksum: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WalletSnapshot {
    pub(super) wallet_fingerprint: String,
    pub(super) network_id: String,
    pub(super) status: &'static str,
    pub(super) generation: u64,
    pub(super) stream_offsets: Vec<StreamOffset>,
    pub(super) unshielded_address: String,
    pub(super) shielded_coin_public_key_hex: String,
    pub(super) shielded_encryption_public_key_hex: String,
    pub(super) dust_public_key: String,
    pub(super) balances: WalletBalanceSnapshot,
    pub(super) pending_submissions: Vec<PendingSubmission>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ApplySyncResult {
    pub(super) duplicate: bool,
    pub(super) snapshot: WalletSnapshot,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) enum SyncRequestMode {
    #[default]
    Standard,
    Fast,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(super) enum RuntimeCommand {
    SignData {
        domain: String,
        data_base64: String,
    },
    CreateCheckPayload {
        preimage_base64: String,
        #[serde(default)]
        ir_base64: Option<String>,
    },
    ParseCheckResult {
        result_base64: String,
    },
    CreateProvingPayload {
        preimage_base64: String,
        #[serde(default)]
        binding_input: Option<String>,
        #[serde(default)]
        key_material: Option<RuntimeProvingKeyMaterial>,
    },
    CanonicalizeTransaction {
        signature_marker: String,
        proof_marker: String,
        binding_marker: String,
        raw_base64: String,
    },
    CreateSyncRequest {
        stream: String,
        from_offset: u64,
        #[serde(default)]
        limit: Option<u64>,
        #[serde(default)]
        mode: SyncRequestMode,
    },
    DeriveShieldedMintContext,
    WatchShieldedMint {
        coin_info_base64: String,
        expected_output_index: u64,
    },
    CreateShieldedSpentRequest,
    ApplyShieldedSpentResponse {
        result_base64: String,
    },
    SetShieldedProtocolVersion {
        protocol_version: u64,
        sync_offset: u64,
    },
    CreateDustSpendRequest,
    CreateDustCommitmentRequest {
        sync_offset: u64,
        raw_base64: String,
    },
    ApplyDustSpendResolution {
        sync_offset: u64,
        raw_base64: String,
        result_base64: String,
    },
    Transfer {
        to: String,
        amount: String,
        token_type: String,
        wallet_type: String,
    },
    DappTransfer {
        outputs: Vec<RuntimeDappOutput>,
    },
    DappIntent {
        inputs: Vec<RuntimeDappInput>,
        outputs: Vec<RuntimeDappOutput>,
    },
    GenerateDust {
        ledger_parameters_base64: String,
        fee_blocks_margin: u64,
        additional_fee_overhead: String,
    },
    BalanceUnsealed {
        raw_base64: String,
        ledger_parameters_base64: String,
        fee_blocks_margin: u64,
        additional_fee_overhead: String,
    },
    BalanceSealed {
        raw_base64: String,
        ledger_parameters_base64: String,
        fee_blocks_margin: u64,
        additional_fee_overhead: String,
    },
    FinalizeUnprovenTransaction {
        raw_base64: String,
        #[serde(default)]
        key_material: Option<BTreeMap<String, RuntimeProvingKeyMaterial>>,
    },
    SubmitFinalized {
        raw_base64: String,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct RuntimeDappInput {
    pub(super) wallet_type: String,
    pub(super) token_type: String,
    pub(super) amount: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct RuntimeDappOutput {
    pub(super) wallet_type: String,
    pub(super) token_type: String,
    pub(super) amount: String,
    pub(super) receiver_address: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct RuntimeProvingKeyMaterial {
    pub(super) prover_key_base64: String,
    pub(super) verifier_key_base64: String,
    pub(super) ir_base64: String,
    #[serde(default)]
    pub(super) compression: Option<String>,
}

impl Drop for RuntimeProvingKeyMaterial {
    fn drop(&mut self) {
        self.prover_key_base64.zeroize();
        self.verifier_key_base64.zeroize();
        self.ir_base64.zeroize();
        if let Some(compression) = &mut self.compression {
            compression.zeroize();
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct NetworkResult {
    pub(super) effect_id: String,
    pub(super) outcome: String,
    #[serde(default)]
    pub(super) body_base64: Option<String>,
}

/// A resume payload carries either one result (every effect except a batched proof
/// round) or one result per effect of a batched proof round. The single form stays
/// wire-identical to the pre-batching protocol.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum NetworkResults {
    Single(NetworkResult),
    Batch(Vec<NetworkResult>),
}

impl NetworkResults {
    pub(super) fn into_vec(self) -> Vec<NetworkResult> {
        match self {
            Self::Single(result) => vec![result],
            Self::Batch(results) => results,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BalanceServiceResult {
    pub(super) tx_hash: String,
    pub(super) tx_bytes: String,
    #[serde(default, deserialize_with = "deserialize_optional_u64")]
    pub(super) expires_at: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct OperationHandle {
    pub(super) id: u64,
    pub(super) generation: u64,
}

/// One effect of a multi-effect round. Emitted only alongside the singular fields, so
/// consumers that predate batching keep reading the first effect and ignore this list.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct OperationEffect {
    pub(super) effect_id: String,
    pub(super) effect: &'static str,
    pub(super) endpoint_role: &'static str,
    pub(super) body_base64: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct OperationStep {
    pub(super) kind: &'static str,
    pub(super) operation: Option<OperationHandle>,
    pub(super) effect_id: Option<String>,
    pub(super) effect: Option<&'static str>,
    pub(super) endpoint_role: Option<&'static str>,
    pub(super) body_base64: Option<String>,
    /// Present only when the round carries two or more effects; the singular fields
    /// above always mirror the first entry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) effects: Option<Vec<OperationEffect>>,
    #[serde(rename = "result", serialize_with = "serialize_embedded_json")]
    pub(super) result_json: Option<String>,
}

pub(super) struct SessionSecrets {
    pub(super) night_external_key: Vec<u8>,
    pub(super) zswap_seed: Vec<u8>,
    pub(super) dust_seed: Vec<u8>,
}
