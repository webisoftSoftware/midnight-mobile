const CHECKPOINT_VERSION: u32 = 1;
const CHECKPOINT_MAGIC: &[u8; 4] = b"MMCP";
const MAX_OPEN_SESSIONS: usize = 2;
const MAX_SYNC_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;
const MAX_CHECKPOINT_BYTES: usize = 64 * 1024 * 1024;
const MAX_SIGNING_DOMAIN_BYTES: usize = 1024;
const MAX_DAPP_SIGN_DATA_BYTES: usize = 1024 * 1024;
const MAX_SYNC_BATCH_RECEIPTS: usize = 4096;
const MAX_CANCELLED_OPERATION_TOMBSTONES: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, uniffi::Record)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSessionHandle {
    pub id: u64,
    pub generation: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WalletSessionConfig {
    network_id: String,
    wallet_fingerprint: String,
    unshielded_address: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StreamOffset {
    stream: String,
    next_offset: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BatchReceipt {
    stream: String,
    from_offset: u64,
    to_offset: u64,
    digest: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum SubmissionStatus {
    AwaitingResponse,
    Accepted,
    Rejected,
    StatusUnknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PendingSubmission {
    transaction_hash: String,
    identifiers: Vec<String>,
    status: SubmissionStatus,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WalletCheckpoint {
    version: u32,
    network_id: String,
    wallet_fingerprint: String,
    legacy_state: LegacyWalletState,
    stream_offsets: Vec<StreamOffset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    caught_up_streams: Vec<String>,
    #[serde(default)]
    batch_receipts: Vec<BatchReceipt>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pending_submissions: Vec<PendingSubmission>,
    ledger_revision: String,
    generation: u64,
    checksum: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WalletSnapshot {
    wallet_fingerprint: String,
    network_id: String,
    status: &'static str,
    generation: u64,
    stream_offsets: Vec<StreamOffset>,
    unshielded_address: String,
    shielded_coin_public_key_hex: String,
    shielded_encryption_public_key_hex: String,
    dust_public_key: String,
    balances: WalletBalanceSnapshot,
    pending_submissions: Vec<PendingSubmission>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApplySyncResult {
    duplicate: bool,
    snapshot: WalletSnapshot,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum RuntimeCommand {
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
        limit: u64,
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
    SubmitFinalized {
        raw_base64: String,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeDappInput {
    wallet_type: String,
    token_type: String,
    amount: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeDappOutput {
    wallet_type: String,
    token_type: String,
    amount: String,
    receiver_address: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeProvingKeyMaterial {
    prover_key_base64: String,
    verifier_key_base64: String,
    ir_base64: String,
    #[serde(default)]
    compression: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NetworkResult {
    effect_id: String,
    outcome: String,
    #[serde(default)]
    body_base64: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BalanceServiceResult {
    tx_hash: String,
    tx_bytes: String,
    #[serde(default, deserialize_with = "deserialize_optional_u64")]
    expires_at: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OperationHandle {
    id: u64,
    generation: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OperationStep {
    kind: &'static str,
    operation: Option<OperationHandle>,
    effect_id: Option<String>,
    effect: Option<&'static str>,
    endpoint_role: Option<&'static str>,
    body_base64: Option<String>,
    #[serde(rename = "result", serialize_with = "serialize_embedded_json")]
    result_json: Option<String>,
}

struct SessionSecrets {
    night_external_key: Vec<u8>,
    zswap_seed: Vec<u8>,
    dust_seed: Vec<u8>,
}
