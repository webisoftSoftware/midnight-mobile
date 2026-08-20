use super::*;

impl NativeWalletState {
    pub(crate) fn validate_wire_offsets(
        stream: &str,
        payloads: &[Vec<u8>],
        from_offset: u64,
        to_offset: u64,
    ) -> Result<(), MidnightRuntimeError> {
        if !matches!(stream, "shielded-wire" | "dust-wire") {
            return Ok(());
        }
        if payloads.is_empty() {
            return Err(MidnightRuntimeError::SyncGap);
        }
        let mut expected = from_offset
            .checked_add(1)
            .ok_or(MidnightRuntimeError::SyncGap)?;
        for payload in payloads {
            let value: serde_json::Value = serde_json::from_slice(payload)
                .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
            let id = value
                .as_object()
                .and_then(|record| record.get("id"))
                .and_then(serde_json::Value::as_u64)
                .ok_or(MidnightRuntimeError::InvalidArgument)?;
            if id != expected {
                return Err(MidnightRuntimeError::SyncGap);
            }
            expected = expected
                .checked_add(1)
                .ok_or(MidnightRuntimeError::SyncGap)?;
        }
        if expected.saturating_sub(1) != to_offset {
            return Err(MidnightRuntimeError::SyncGap);
        }
        Ok(())
    }

    pub(crate) fn validate_v2_trailer(
        stream: &str,
        payloads: &[Vec<u8>],
        to_offset: u64,
    ) -> Result<(), MidnightRuntimeError> {
        if !matches!(stream, "shielded-v2" | "dust-v2") {
            return Ok(());
        }
        let [payload] = payloads else {
            return Err(MidnightRuntimeError::InvalidArgument);
        };
        let trailer = payload
            .len()
            .checked_sub(8)
            .and_then(|start| payload.get(start..))
            .and_then(|bytes| bytes.try_into().ok())
            .map(u64::from_le_bytes)
            .ok_or(MidnightRuntimeError::InvalidArgument)?;
        if trailer == 0 || trailer != to_offset {
            return Err(MidnightRuntimeError::SyncGap);
        }
        Ok(())
    }

    pub(crate) fn snapshot_sync_request(
        stream: &str,
        zswap_seed: &[u8],
        dust_seed: &[u8],
    ) -> Result<Vec<u8>, MidnightRuntimeError> {
        match stream {
            "shielded" => {
                let mut seed: [u8; 32] = zswap_seed
                    .try_into()
                    .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
                let keys = ZswapSecretKeys::from(ZswapSeed::from(seed));
                seed.zeroize();
                let mut viewing_key = Vec::new();
                tagged_serialize(&keys.encryption_secret_key, &mut viewing_key)
                    .map_err(|_| MidnightRuntimeError::NativeInternal)?;
                serde_json::to_vec(&serde_json::json!({
                    "viewingKey": hex::encode(viewing_key),
                    "coinPublicKey": serializable_hex(&keys.coin_public_key())?,
                }))
                .map_err(|_| MidnightRuntimeError::NativeInternal)
            }
            "dust" => {
                let mut seed: [u8; 32] = dust_seed
                    .try_into()
                    .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
                let secret_key = DustSecretKey::derive_secret_key(&seed);
                seed.zeroize();
                let public_key = DustPublicKey::from(secret_key);
                let public_key =
                    BigUint::from_bytes_le(&public_key.0.as_le_bytes()).to_str_radix(16);
                serde_json::to_vec(&serde_json::json!({
                    "dustPublicKey": format!("{public_key:0>64}"),
                }))
                .map_err(|_| MidnightRuntimeError::NativeInternal)
            }
            _ => Err(MidnightRuntimeError::InvalidArgument),
        }
    }

    pub(crate) fn shielded_spent_request(&self) -> Result<Vec<u8>, MidnightRuntimeError> {
        let mut nullifier_prefixes = Vec::new();
        for (_, coin) in self.shielded.coins.iter() {
            let nonce = serializable_hex(&coin.nonce)?;
            let hashes = self
                .coin_hashes
                .get(&nonce)
                .ok_or(MidnightRuntimeError::StateIncompatible)?;
            if hashes.nullifier.len() != 64
                || !hashes
                    .nullifier
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(MidnightRuntimeError::StateIncompatible);
            }
            nullifier_prefixes.push(hashes.nullifier[..32].to_ascii_lowercase());
        }
        nullifier_prefixes.sort();
        nullifier_prefixes.dedup();
        serde_json::to_vec(&serde_json::json!({
            "nullifierPrefixes": nullifier_prefixes,
        }))
        .map_err(|_| MidnightRuntimeError::NativeInternal)
    }

    pub(crate) fn apply_shielded_spent_response(
        &self,
        payload: &[u8],
        zswap_seed: &[u8],
    ) -> Result<(Self, usize), MidnightRuntimeError> {
        let response: ShieldedSpentResponse =
            serde_json::from_slice(payload).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
        if response.results.len() > MAX_SYNC_ENTRIES {
            return Err(MidnightRuntimeError::InvalidArgument);
        }

        let mut seed: [u8; 32] = zswap_seed
            .try_into()
            .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
        let keys = ZswapSecretKeys::from(ZswapSeed::from(seed));
        seed.zeroize();
        let mut known = BTreeMap::<String, ShieldedNullifier>::new();
        for (nullifier, coin) in self.shielded.coins.iter() {
            let computed = ShieldedCoinInfo::from(&*coin)
                .nullifier(&SenderEvidence::User(Cow::Borrowed(&keys.coin_secret_key)));
            if computed != nullifier {
                return Err(MidnightRuntimeError::StateIncompatible);
            }
            known.insert(serializable_hex(&computed)?, computed);
        }

        let mut requested = Vec::with_capacity(response.results.len());
        for result in response.results {
            let value = match result {
                ShieldedSpentResult::Nullifier(value) => value,
                ShieldedSpentResult::Record(record) => record.nullifier,
            };
            if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(MidnightRuntimeError::InvalidArgument);
            }
            let value = value.to_ascii_lowercase();
            if known.contains_key(&value) {
                requested.push(value);
            }
        }
        requested.sort();
        requested.dedup();

        let mut proposed = self.clone();
        for value in &requested {
            let nullifier = known
                .get(value)
                .copied()
                .ok_or(MidnightRuntimeError::NativeInternal)?;
            proposed.shielded = proposed.shielded.remove_coin_by_nullifier(nullifier);
        }
        proposed.refresh_coin_hashes(&keys)?;
        Ok((proposed, requested.len()))
    }

    pub(crate) fn set_shielded_protocol_version(
        &self,
        protocol_version: u64,
    ) -> Result<Self, MidnightRuntimeError> {
        if protocol_version == 0 {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        let mut proposed = self.clone();
        proposed.protocol_version = protocol_version.to_string();
        Ok(proposed)
    }

    fn resolve_dust_spend_chains(
        &self,
        payload: &[u8],
        expected_event_id: u64,
        dust_seed: &[u8],
    ) -> Result<DustSpendResolution, MidnightRuntimeError> {
        let (spend_event_id, mut records) = dust_spend_records(payload)?;
        if spend_event_id < expected_event_id {
            return Err(MidnightRuntimeError::SyncGap);
        }
        if spend_event_id > expected_event_id {
            return Ok(DustSpendResolution::Ahead);
        }

        let initial = self.dust.utxos().collect::<Vec<_>>();
        if initial.is_empty() {
            return Ok(DustSpendResolution::Unchanged);
        }
        let mut seed: [u8; 32] = dust_seed
            .try_into()
            .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
        let secret_key = DustSecretKey::derive_secret_key(&seed);
        seed.zeroize();
        let mut final_utxos = Vec::with_capacity(initial.len());
        let mut changed = false;

        for initial_utxo in &initial {
            let generation = self
                .dust
                .generation_info(initial_utxo)
                .ok_or(MidnightRuntimeError::StateIncompatible)?;
            let mut current = *initial_utxo;
            loop {
                let nullifier = current.nullifier(&secret_key);
                let nullifier_bytes = nullifier.0.as_le_bytes();
                let prefix: [u8; 16] = nullifier_bytes[..16]
                    .try_into()
                    .map_err(|_| MidnightRuntimeError::NativeInternal)?;
                let Some(record) = records.remove(&prefix) else {
                    break;
                };
                current = successor_utxo(
                    &current,
                    &midnight_base_crypto::time::Timestamp::from_secs(record.declared_time),
                    record.v_fee,
                    record.commitment_index,
                    &generation,
                    &secret_key,
                    &self.dust.params,
                );
                changed = true;
            }
            final_utxos.push(current);
        }
        if !changed {
            return Ok(DustSpendResolution::Unchanged);
        }

        let mut resolved = self.dust.clone();
        for utxo in &initial {
            resolved = resolved
                .remove_utxo(&utxo.nullifier(&secret_key))
                .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
        }
        let mut final_nullifiers = BTreeMap::new();
        for utxo in &final_utxos {
            let nullifier = utxo.nullifier(&secret_key);
            let nullifier_hex = serializable_hex(&nullifier)?;
            if final_nullifiers.insert(nullifier_hex, ()).is_some() {
                return Err(MidnightRuntimeError::StateIncompatible);
            }
            resolved = resolved
                .add_utxo(&nullifier, utxo, None)
                .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
        }
        Ok(DustSpendResolution::Changed(Box::new(resolved)))
    }

    pub(crate) fn dust_spend_request(
        &self,
        dust_seed: &[u8],
    ) -> Result<(Vec<u8>, usize), MidnightRuntimeError> {
        let mut seed: [u8; 32] = dust_seed
            .try_into()
            .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
        let secret_key = DustSecretKey::derive_secret_key(&seed);
        seed.zeroize();
        let mut nullifier_prefixes = self
            .dust
            .utxos()
            .map(|utxo| serializable_hex(&utxo.nullifier(&secret_key)))
            .collect::<Result<Vec<_>, _>>()?;
        for nullifier in &mut nullifier_prefixes {
            nullifier.truncate(32);
        }
        nullifier_prefixes.sort();
        nullifier_prefixes.dedup();
        let utxo_count = self.dust.utxos().count();
        let body = serde_json::to_vec(&serde_json::json!({
            "nullifierPrefixes": nullifier_prefixes,
        }))
        .map_err(|_| MidnightRuntimeError::NativeInternal)?;
        Ok((body, utxo_count))
    }

    pub(crate) fn dust_commitment_request(
        &self,
        payload: &[u8],
        expected_event_id: u64,
        dust_seed: &[u8],
    ) -> Result<DustCommitmentRequest, MidnightRuntimeError> {
        let resolved =
            match self.resolve_dust_spend_chains(payload, expected_event_id, dust_seed)? {
                DustSpendResolution::Ahead => return Ok(DustCommitmentRequest::Ahead),
                DustSpendResolution::Unchanged => return Ok(DustCommitmentRequest::Unchanged),
                DustSpendResolution::Changed(resolved) => resolved,
            };

        let mut seed: [u8; 32] = dust_seed
            .try_into()
            .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
        let secret_key = DustSecretKey::derive_secret_key(&seed);
        seed.zeroize();
        let public_key = DustPublicKey::from(secret_key);
        let public_key = BigUint::from_bytes_le(&public_key.0.as_le_bytes()).to_str_radix(16);
        let mut positions = Vec::new();
        for utxo in resolved.utxos() {
            let mut raw = Vec::new();
            utxo.serialize(&mut raw)
                .map_err(|_| MidnightRuntimeError::NativeInternal)?;
            positions.push(serde_json::json!({
                "genIndex": 0,
                "mtIndex": utxo.mt_index,
                "utxo": hex::encode(raw),
            }));
        }
        positions.sort_by_key(|position| {
            position
                .get("mtIndex")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(u64::MAX)
        });
        if positions.is_empty() {
            return Err(MidnightRuntimeError::StateIncompatible);
        }
        let body = serde_json::to_vec(&serde_json::json!({
            "dustPublicKey": format!("{public_key:0>64}"),
            "comPositions": positions,
        }))
        .map_err(|_| MidnightRuntimeError::NativeInternal)?;
        Ok(DustCommitmentRequest::Rebuild(body))
    }

    pub(crate) fn apply_dust_spend_resolution(
        &self,
        spend_payload: &[u8],
        commitment_response: &[u8],
        expected_event_id: u64,
        dust_seed: &[u8],
    ) -> Result<Self, MidnightRuntimeError> {
        let resolved =
            match self.resolve_dust_spend_chains(spend_payload, expected_event_id, dust_seed)? {
                DustSpendResolution::Changed(resolved) => resolved,
                _ => return Err(MidnightRuntimeError::SyncGap),
            };
        let payload = dust_commitment_response_payload(commitment_response, expected_event_id)?;
        let held = resolved
            .utxos()
            .map(|utxo| (utxo.mt_index, utxo))
            .collect::<BTreeMap<_, _>>();
        if held.len() != resolved.utxos().count() {
            return Err(MidnightRuntimeError::StateIncompatible);
        }

        let mut rebuilt = (*resolved).clone();
        rebuilt.commitment_tree = MerkleTree::blank(DUST_COMMITMENT_TREE_DEPTH);
        rebuilt.commitment_tree_first_free = 0;
        let mut remaining = held;
        let mut cursor = BinaryCursor::new(payload);
        let count =
            usize::try_from(cursor.u32_le()?).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
        if count > MAX_SYNC_ENTRIES {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        for _ in 0..count {
            let collapsed = cursor.length_prefixed()?;
            if !collapsed.is_empty() {
                let update: MerkleTreeCollapsedUpdate = tagged_deserialize(&mut &collapsed[..])
                    .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
                rebuilt = rebuilt
                    .apply_commitment_collapsed_update(&update)
                    .map_err(|_| MidnightRuntimeError::SyncGap)?;
            }
            // Validate the placeholder framing, but always insert the locally
            // derived QDO so transport can never replace owned wallet state.
            let placeholder: QualifiedDustOutput =
                deserialize_tagged_or_plain(cursor.length_prefixed()?)?;
            let index = cursor.u64_le()?;
            if placeholder.mt_index != index {
                return Err(MidnightRuntimeError::InvalidArgument);
            }
            let utxo = remaining
                .remove(&index)
                .ok_or(MidnightRuntimeError::StateIncompatible)?;
            rebuilt = rebuilt
                .insert_commitment(index, utxo, true)
                .map_err(|_| MidnightRuntimeError::SyncGap)?;
        }
        let trailing = cursor.length_prefixed()?;
        if !trailing.is_empty() {
            let update: MerkleTreeCollapsedUpdate = tagged_deserialize(&mut &trailing[..])
                .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
            rebuilt = rebuilt
                .apply_commitment_collapsed_update(&update)
                .map_err(|_| MidnightRuntimeError::SyncGap)?;
        }
        if cursor.remaining() != 0 || !remaining.is_empty() {
            return Err(MidnightRuntimeError::StateIncompatible);
        }
        let mut proposed = self.clone();
        proposed.dust = rebuilt;
        Ok(proposed)
    }
}
