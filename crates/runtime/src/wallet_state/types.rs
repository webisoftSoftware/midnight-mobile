use super::*;

pub(super) struct BinaryCursor<'a> {
    pub(super) bytes: &'a [u8],
    pub(super) offset: usize,
}

impl<'a> BinaryCursor<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(super) fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    pub(super) fn take(&mut self, length: usize) -> Result<&'a [u8], MidnightRuntimeError> {
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

    pub(super) fn u32_le(&mut self) -> Result<u32, MidnightRuntimeError> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
        Ok(u32::from_le_bytes(bytes))
    }

    pub(super) fn u64_le(&mut self) -> Result<u64, MidnightRuntimeError> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
        Ok(u64::from_le_bytes(bytes))
    }

    pub(super) fn length_prefixed(&mut self) -> Result<&'a [u8], MidnightRuntimeError> {
        let length =
            usize::try_from(self.u32_le()?).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
        self.take(length)
    }
}

pub(super) fn deserialize_tagged_or_plain<T>(bytes: &[u8]) -> Result<T, MidnightRuntimeError>
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
pub(super) struct ShieldedPublicKeys {
    pub(super) coin_public_key: String,
    pub(super) encryption_public_key: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CoinHashes {
    pub(super) nullifier: String,
    pub(super) commitment: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum ShieldedSpentResult {
    Nullifier(String),
    Record(ShieldedSpentRecord),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ShieldedSpentRecord {
    pub(super) nullifier: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ShieldedSpentResponse {
    pub(super) results: Vec<ShieldedSpentResult>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct DustSpendRecord {
    pub(super) commitment_index: u64,
    pub(super) v_fee: u128,
    pub(super) declared_time: u64,
}

pub(super) enum DustSpendResolution {
    Ahead,
    Unchanged,
    Changed(Box<DustLocalState<InMemoryDB>>),
}

pub(crate) enum DustCommitmentRequest {
    Ahead,
    Unchanged,
    Rebuild(Vec<u8>),
}

pub(super) fn dust_spend_records(
    payload: &[u8],
) -> Result<(u64, BTreeMap<[u8; 16], DustSpendRecord>), MidnightRuntimeError> {
    if payload.len() < DUST_SPEND_HEADER_BYTES {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    let mut cursor = BinaryCursor::new(payload);
    let last_event_id = cursor.u64_le()?;
    let count =
        usize::try_from(cursor.u32_le()?).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
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

pub(super) fn inspect_dust_commitment_payload(payload: &[u8]) -> Option<()> {
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

pub(super) fn dust_interleaved_commitment_offset(payload: &[u8]) -> Option<usize> {
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

pub(super) fn dust_commitment_response_payload(
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
pub(super) struct ShieldedSnapshot {
    pub(super) public_keys: ShieldedPublicKeys,
    pub(super) state: String,
    pub(super) protocol_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) offset: Option<String>,
    pub(super) network_id: String,
    pub(super) coin_hashes: BTreeMap<String, CoinHashes>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DustPublicKeySnapshot {
    pub(super) public_key: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DustSnapshot {
    pub(super) public_key: DustPublicKeySnapshot,
    pub(super) state: String,
    pub(super) protocol_version: String,
    pub(super) network_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) offset: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct UnshieldedPublicKey {
    pub(super) public_key: String,
    pub(super) address_hex: String,
    pub(super) address: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct UnshieldedUtxo {
    pub(super) value: String,
    pub(super) owner: String,
    #[serde(rename = "type")]
    pub(super) type_: String,
    pub(super) intent_hash: String,
    pub(super) output_no: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct UnshieldedMeta {
    pub(super) ctime: i64,
    pub(super) registered_for_dust_generation: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct UnshieldedUtxoWithMeta {
    pub(super) utxo: UnshieldedUtxo,
    pub(super) meta: UnshieldedMeta,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct UnshieldedCollections {
    pub(super) available_utxos: Vec<UnshieldedUtxoWithMeta>,
    pub(super) pending_utxos: Vec<UnshieldedUtxoWithMeta>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct UnshieldedSnapshot {
    pub(super) public_key: UnshieldedPublicKey,
    pub(super) state: UnshieldedCollections,
    pub(super) protocol_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) applied_id: Option<String>,
    pub(super) network_id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct WireUtxo {
    pub(super) value: String,
    pub(super) owner: String,
    pub(super) token_type: String,
    pub(super) intent_hash: String,
    pub(super) output_index: u32,
    pub(super) ctime: Option<i64>,
    pub(super) registered_for_dust_generation: bool,
}

impl WireUtxo {
    pub(super) fn into_legacy(
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
    pub(super) nonce: String,
    pub(super) generated_now: String,
    pub(super) max_cap: String,
    pub(super) max_cap_reached_at: Option<String>,
    pub(super) maturing: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WalletBalanceSnapshot {
    pub(crate) shielded_balances: BTreeMap<String, String>,
    pub(crate) unshielded_balances: BTreeMap<String, String>,
    pub(super) total_shielded: String,
    pub(super) total_unshielded: String,
    pub(super) total_shielded_all: String,
    pub(super) total_unshielded_all: String,
    pub(super) dust_balance: String,
    pub(super) dust_coins: Vec<DustCoinSnapshot>,
    pub(super) available_utxos: u64,
    pub(super) dust_generating_night: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct WireTransactionResult {
    pub(super) status: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct WireBlock {
    pub(super) timestamp: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ShieldedWireEvent {
    pub(super) id: u64,
    pub(super) raw: String,
    pub(super) protocol_version: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DustWireEvent {
    pub(super) id: u64,
    pub(super) raw: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct WireTransaction {
    pub(super) id: u64,
    #[serde(rename = "type")]
    pub(super) type_: String,
    #[serde(default)]
    pub(super) block: Option<WireBlock>,
    #[serde(default)]
    pub(super) transaction_result: Option<WireTransactionResult>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type")]
pub(super) enum UnshieldedSyncUpdate {
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
