struct BinaryCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> BinaryCursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], MidnightRuntimeError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(MidnightRuntimeError::InvalidArgument)?;
        if end > self.bytes.len() {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        let result = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(result)
    }

    fn u32_le(&mut self) -> Result<u32, MidnightRuntimeError> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn u64_le(&mut self) -> Result<u64, MidnightRuntimeError> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
        Ok(u64::from_le_bytes(bytes))
    }

    fn length_prefixed(&mut self) -> Result<&'a [u8], MidnightRuntimeError> {
        let length =
            usize::try_from(self.u32_le()?).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
        self.take(length)
    }
}

fn deserialize_tagged_or_plain<T>(bytes: &[u8]) -> Result<T, MidnightRuntimeError>
where
    T: Deserializable + Serializable + Tagged,
{
    if let Ok(value) = tagged_deserialize(&mut &bytes[..]) {
        return Ok(value);
    }
    T::deserialize(&mut &bytes[..], 0).map_err(|_| MidnightRuntimeError::InvalidArgument)
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LegacyWalletState {
    pub serialized_dust_wallet_state: String,
    pub serialized_shielded_wallet_state: String,
    pub serialized_unshielded_wallet_state: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ShieldedPublicKeys {
    coin_public_key: String,
    encryption_public_key: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CoinHashes {
    nullifier: String,
    commitment: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum ShieldedSpentResult {
    Nullifier(String),
    Record(ShieldedSpentRecord),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShieldedSpentRecord {
    nullifier: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShieldedSpentResponse {
    results: Vec<ShieldedSpentResult>,
}

#[derive(Clone, Copy, Debug)]
struct DustSpendRecord {
    commitment_index: u64,
    v_fee: u128,
    declared_time: u64,
}

enum DustSpendResolution {
    Ahead,
    Unchanged,
    Changed(Box<DustLocalState<InMemoryDB>>),
}

pub(crate) enum DustCommitmentRequest {
    Ahead,
    Unchanged,
    Rebuild(Vec<u8>),
}

fn dust_spend_records(
    payload: &[u8],
) -> Result<(u64, BTreeMap<[u8; 16], DustSpendRecord>), MidnightRuntimeError> {
    if payload.len() < DUST_SPEND_HEADER_BYTES {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let mut cursor = BinaryCursor::new(payload);
    let last_event_id = cursor.u64_le()?;
    let count = usize::try_from(cursor.u32_le()?).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
    if last_event_id == 0 || count > MAX_SYNC_ENTRIES {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    if count == 0 {
        if cursor.remaining() != 0 {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        return Ok((last_event_id, BTreeMap::new()));
    }
    if !cursor.remaining().is_multiple_of(count) {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let record_size = cursor.remaining() / count;
    if record_size != DUST_SPEND_WASM_RECORD_BYTES
        && record_size != DUST_SPEND_EXTENDED_RECORD_BYTES
    {
        return Err(MidnightRuntimeError::InvalidArgument);
    }

    let mut records = BTreeMap::new();
    for _ in 0..count {
        let record = cursor.take(record_size)?;
        let prefix: [u8; 16] = record[..16]
            .try_into()
            .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
        let commitment_index = u64::from_le_bytes(
            record[16..24]
                .try_into()
                .map_err(|_| MidnightRuntimeError::InvalidArgument)?,
        );
        let v_fee = u64::from_le_bytes(
            record[24..32]
                .try_into()
                .map_err(|_| MidnightRuntimeError::InvalidArgument)?,
        ) as u128;
        let declared_time = if record_size == DUST_SPEND_WASM_RECORD_BYTES {
            u64::from(u32::from_le_bytes(
                record[32..36]
                    .try_into()
                    .map_err(|_| MidnightRuntimeError::InvalidArgument)?,
            ))
        } else {
            u64::from_le_bytes(
                record[40..48]
                    .try_into()
                    .map_err(|_| MidnightRuntimeError::InvalidArgument)?,
            )
        };
        if records
            .insert(
                prefix,
                DustSpendRecord {
                    commitment_index,
                    v_fee,
                    declared_time,
                },
            )
            .is_some()
        {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
    }
    if cursor.remaining() != 0 {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    Ok((last_event_id, records))
}

fn inspect_dust_commitment_payload(payload: &[u8]) -> Option<()> {
    let mut cursor = BinaryCursor::new(payload);
    let count = usize::try_from(cursor.u32_le().ok()?).ok()?;
    if count > MAX_SYNC_ENTRIES {
        return None;
    }
    for _ in 0..count {
        cursor.length_prefixed().ok()?;
        cursor.length_prefixed().ok()?;
        cursor.u64_le().ok()?;
    }
    cursor.length_prefixed().ok()?;
    (cursor.remaining() == 0).then_some(())
}

fn dust_interleaved_commitment_offset(payload: &[u8]) -> Option<usize> {
    let mut cursor = BinaryCursor::new(payload);
    let count = usize::try_from(cursor.u32_le().ok()?).ok()?;
    if count > MAX_SYNC_ENTRIES {
        return None;
    }
    for _ in 0..count {
        cursor.length_prefixed().ok()?;
        cursor.length_prefixed().ok()?;
        cursor.u64_le().ok()?;
    }
    cursor.length_prefixed().ok()?;
    let offset = cursor.offset;
    inspect_dust_commitment_payload(&payload[offset..])?;
    Some(offset)
}

fn dust_commitment_response_payload(
    response: &[u8],
    expected_event_id: u64,
) -> Result<&[u8], MidnightRuntimeError> {
    if response.len() < 12 {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let body_end = response
        .len()
        .checked_sub(8)
        .ok_or(MidnightRuntimeError::InvalidArgument)?;
    let trailer = u64::from_le_bytes(
        response[body_end..]
            .try_into()
            .map_err(|_| MidnightRuntimeError::InvalidArgument)?,
    );
    if trailer != expected_event_id {
        return Err(MidnightRuntimeError::SyncGap);
    }

    let params_len = response
        .get(..4)
        .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
        .map(u32::from_le_bytes)
        .and_then(|length| usize::try_from(length).ok())
        .ok_or(MidnightRuntimeError::InvalidArgument)?;
    let mut candidates = Vec::with_capacity(3);
    if params_len > 0 {
        if let Some(offset) = 4_usize
            .checked_add(params_len)
            .and_then(|offset| offset.checked_add(8))
            .filter(|offset| *offset <= body_end)
        {
            candidates.push(offset);
        }
    } else if 12 <= body_end {
        candidates.push(12);
    }
    candidates.push(0);

    for candidate in candidates {
        let framed = &response[candidate..body_end];
        if inspect_dust_commitment_payload(framed).is_some() {
            return Ok(framed);
        }
        if let Some(offset) = dust_interleaved_commitment_offset(framed) {
            return Ok(&framed[offset..]);
        }
    }
    Err(MidnightRuntimeError::InvalidArgument)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ShieldedSnapshot {
    public_keys: ShieldedPublicKeys,
    state: String,
    protocol_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    offset: Option<String>,
    network_id: String,
    coin_hashes: BTreeMap<String, CoinHashes>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DustPublicKeySnapshot {
    public_key: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DustSnapshot {
    public_key: DustPublicKeySnapshot,
    state: String,
    protocol_version: String,
    network_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    offset: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UnshieldedPublicKey {
    public_key: String,
    address_hex: String,
    address: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UnshieldedUtxo {
    value: String,
    owner: String,
    #[serde(rename = "type")]
    type_: String,
    intent_hash: String,
    output_no: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UnshieldedMeta {
    ctime: i64,
    registered_for_dust_generation: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UnshieldedUtxoWithMeta {
    utxo: UnshieldedUtxo,
    meta: UnshieldedMeta,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UnshieldedCollections {
    available_utxos: Vec<UnshieldedUtxoWithMeta>,
    pending_utxos: Vec<UnshieldedUtxoWithMeta>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UnshieldedSnapshot {
    public_key: UnshieldedPublicKey,
    state: UnshieldedCollections,
    protocol_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    applied_id: Option<String>,
    network_id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireUtxo {
    value: String,
    owner: String,
    token_type: String,
    intent_hash: String,
    output_index: u32,
    ctime: Option<i64>,
    registered_for_dust_generation: bool,
}

impl WireUtxo {
    fn into_legacy(
        self,
        transaction_timestamp_ms: Option<i64>,
    ) -> Result<UnshieldedUtxoWithMeta, MidnightRuntimeError> {
        if !canonical_decimal(&self.value)
            || self.value.parse::<u128>().is_err()
            || !nonempty_bounded(&self.owner, 512)
            || !nonempty_bounded(&self.token_type, 512)
            || !nonempty_bounded(&self.intent_hash, 512)
        {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        let ctime_ms = match self.ctime {
            Some(value) => value
                .checked_mul(1_000)
                .ok_or(MidnightRuntimeError::InvalidArgument)?,
            None if !self.registered_for_dust_generation => transaction_timestamp_ms
                .filter(|value| *value >= 0)
                .ok_or(MidnightRuntimeError::InvalidArgument)?,
            None => return Err(MidnightRuntimeError::InvalidArgument),
        };
        Ok(UnshieldedUtxoWithMeta {
            utxo: UnshieldedUtxo {
                value: self.value,
                owner: self.owner,
                type_: self.token_type,
                intent_hash: self.intent_hash,
                output_no: self.output_index,
            },
            meta: UnshieldedMeta {
                ctime: ctime_ms,
                registered_for_dust_generation: self.registered_for_dust_generation,
            },
        })
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DustCoinSnapshot {
    nonce: String,
    generated_now: String,
    max_cap: String,
    max_cap_reached_at: Option<String>,
    maturing: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WalletBalanceSnapshot {
    pub(crate) shielded_balances: BTreeMap<String, String>,
    pub(crate) unshielded_balances: BTreeMap<String, String>,
    total_shielded: String,
    total_unshielded: String,
    total_shielded_all: String,
    total_unshielded_all: String,
    dust_balance: String,
    dust_coins: Vec<DustCoinSnapshot>,
    available_utxos: u64,
    dust_generating_night: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireTransactionResult {
    status: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireBlock {
    timestamp: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ShieldedWireEvent {
    id: u64,
    raw: String,
    protocol_version: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DustWireEvent {
    id: u64,
    raw: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireTransaction {
    id: u64,
    #[serde(rename = "type")]
    type_: String,
    #[serde(default)]
    block: Option<WireBlock>,
    #[serde(default)]
    transaction_result: Option<WireTransactionResult>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type")]
enum UnshieldedSyncUpdate {
    UnshieldedTransaction {
        transaction: WireTransaction,
        #[serde(rename = "createdUtxos")]
        created_utxos: Vec<WireUtxo>,
        #[serde(rename = "spentUtxos")]
        spent_utxos: Vec<WireUtxo>,
    },
    UnshieldedTransactionsProgress {
        #[serde(rename = "highestTransactionId")]
        highest_transaction_id: u64,
    },
}
