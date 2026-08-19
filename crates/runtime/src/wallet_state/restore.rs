use super::*;

impl NativeWalletState {
    pub(crate) fn restore(
        legacy: &LegacyWalletState,
        context: RestoreContext<'_>,
    ) -> Result<Self, MidnightRuntimeError> {
        let signing_key = SigningKey::from_bytes(context.night_external_key)
            .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
        let verifying_key = serializable_hex(&signing_key.verifying_key())?;

        let mut result = Self {
            shielded: midnight_zswap::local::State::new(),
            dust: DustLocalState::new(INITIAL_DUST_PARAMETERS),
            unshielded: UnshieldedCollections::default(),
            coin_hashes: BTreeMap::new(),
            protocol_version: "0".to_owned(),
        };

        if !legacy.serialized_shielded_wallet_state.is_empty() {
            let snapshot: ShieldedSnapshot =
                parse_snapshot(&legacy.serialized_shielded_wallet_state)?;
            if snapshot.network_id != context.network_id
                || snapshot.public_keys.coin_public_key
                    != context.address_material.shielded_coin_public_key_hex
                || snapshot.public_keys.encryption_public_key
                    != context.address_material.shielded_encryption_public_key_hex
                || !canonical_decimal(&snapshot.protocol_version)
            {
                return Err(MidnightRuntimeError::StateIncompatible);
            }
            result.shielded = decode_hex_state(&snapshot.state)?;
            result.coin_hashes = snapshot.coin_hashes;
            result.protocol_version = snapshot.protocol_version;
        }

        if !legacy.serialized_dust_wallet_state.is_empty() {
            let snapshot: DustSnapshot = parse_snapshot(&legacy.serialized_dust_wallet_state)?;
            if snapshot.network_id != context.network_id
                || snapshot.public_key.public_key != context.address_material.dust_public_key
                || !canonical_decimal(&snapshot.protocol_version)
                || snapshot.protocol_version != result.protocol_version
            {
                return Err(MidnightRuntimeError::StateIncompatible);
            }
            result.dust = decode_hex_state(&snapshot.state)?;
        }

        if !legacy.serialized_unshielded_wallet_state.is_empty() {
            let snapshot: UnshieldedSnapshot =
                parse_snapshot(&legacy.serialized_unshielded_wallet_state)?;
            if snapshot.network_id != context.network_id
                || snapshot.public_key.public_key != verifying_key
                || snapshot.public_key.address_hex
                    != context.address_material.unshielded_address_hex
                || snapshot.public_key.address != context.unshielded_address
                || !canonical_decimal(&snapshot.protocol_version)
                || snapshot.protocol_version != result.protocol_version
            {
                return Err(MidnightRuntimeError::StateIncompatible);
            }
            result.unshielded = snapshot.state;
        }

        result.validate_coin_hash_keys()?;
        Ok(result)
    }

    pub(crate) fn balances(&self) -> Result<WalletBalanceSnapshot, MidnightRuntimeError> {
        const NIGHT_TOKEN: &str =
            "0000000000000000000000000000000000000000000000000000000000000000";
        let mut shielded = BTreeMap::<String, u128>::new();
        for (nullifier, coin) in self.shielded.coins.iter() {
            if self.shielded.pending_spends.contains_key(&nullifier) {
                continue;
            }
            add_balance(&mut shielded, serializable_hex(&coin.type_.0)?, coin.value);
        }
        for (_, coin) in self.shielded.pending_outputs.iter() {
            add_balance(&mut shielded, serializable_hex(&coin.type_.0)?, coin.value);
        }

        let mut unshielded = BTreeMap::<String, u128>::new();
        let mut available_utxos = 0_u64;
        let mut dust_generating_night = 0_u128;
        for coin in self
            .unshielded
            .available_utxos
            .iter()
            .chain(self.unshielded.pending_utxos.iter())
        {
            let value = coin
                .utxo
                .value
                .parse::<u128>()
                .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
            add_balance(&mut unshielded, coin.utxo.type_.clone(), value);
            if coin.utxo.type_ == NIGHT_TOKEN && coin.meta.registered_for_dust_generation {
                dust_generating_night = dust_generating_night.saturating_add(value);
            }
        }
        for coin in &self.unshielded.available_utxos {
            if coin.utxo.type_ == NIGHT_TOKEN && !coin.meta.registered_for_dust_generation {
                available_utxos = available_utxos.saturating_add(1);
            }
        }

        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let now = midnight_base_crypto::time::Timestamp::from_secs(now_secs);
        let dust_balance = self.dust.wallet_balance(now);
        let mut dust_coins = self
            .dust
            .utxos()
            .filter_map(|coin| {
                let generation = self.dust.generation_info(&coin)?;
                let generated = midnight_ledger::dust::DustOutput::from(coin).updated_value(
                    &generation,
                    now,
                    &self.dust.params,
                );
                let max_cap = generation
                    .value
                    .saturating_mul(self.dust.params.night_dust_ratio as u128);
                Some(DustCoinSnapshot {
                    nonce: num_bigint::BigUint::from_bytes_le(&coin.nonce.as_le_bytes())
                        .to_str_radix(10),
                    generated_now: generated.to_string(),
                    max_cap: max_cap.to_string(),
                    // UI treats this as optional; computing the value itself is
                    // native and exact, while presentation of wall-clock dates
                    // remains an RN concern.
                    max_cap_reached_at: None,
                    maturing: generated < max_cap,
                })
            })
            .collect::<Vec<_>>();
        dust_coins.sort_by(|left, right| left.nonce.cmp(&right.nonce));

        let total_shielded_all = shielded
            .values()
            .fold(0_u128, |total, value| total.saturating_add(*value));
        let total_unshielded_all = unshielded
            .values()
            .fold(0_u128, |total, value| total.saturating_add(*value));
        Ok(WalletBalanceSnapshot {
            total_shielded: shielded.get(NIGHT_TOKEN).copied().unwrap_or(0).to_string(),
            total_unshielded: unshielded
                .get(NIGHT_TOKEN)
                .copied()
                .unwrap_or(0)
                .to_string(),
            total_shielded_all: total_shielded_all.to_string(),
            total_unshielded_all: total_unshielded_all.to_string(),
            shielded_balances: decimal_balances(shielded),
            unshielded_balances: decimal_balances(unshielded),
            dust_balance: dust_balance.to_string(),
            dust_coins,
            available_utxos,
            dust_generating_night: dust_generating_night.to_string(),
        })
    }

    pub(super) fn import_shielded_v2(
        &self,
        payload: &[u8],
        zswap_seed: &[u8],
    ) -> Result<Self, MidnightRuntimeError> {
        let mut seed: [u8; 32] = zswap_seed
            .try_into()
            .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
        let keys = ZswapSecretKeys::from(ZswapSeed::from(seed));
        seed.zeroize();
        let mut cursor = BinaryCursor::new(payload);
        let segment_count =
            usize::try_from(cursor.u32_le()?).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
        if segment_count > MAX_SYNC_ENTRIES {
            return Err(MidnightRuntimeError::InvalidArgument);
        }

        let mut shielded = midnight_zswap::local::State::new();
        for _ in 0..segment_count {
            let collapsed = cursor.length_prefixed()?;
            if !collapsed.is_empty() {
                let update: MerkleTreeCollapsedUpdate = tagged_deserialize(&mut &collapsed[..])
                    .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
                shielded = shielded
                    .apply_collapsed_update(&update)
                    .map_err(|_| MidnightRuntimeError::SyncGap)?;
            }
            let record = cursor.length_prefixed()?;
            let _diagnostic_index = cursor.u64_le()?;
            if record.len() > V2_SHIELDED_RECORD_HEADER_BYTES {
                let events: Vec<Event<InMemoryDB>> =
                    tagged_deserialize_sequence(&record[V2_SHIELDED_RECORD_HEADER_BYTES..])
                        .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
                shielded = shielded
                    .replay_events(&keys, events.iter())
                    .map_err(|_| MidnightRuntimeError::SyncGap)?;
            } else if record.len() < V2_SHIELDED_RECORD_HEADER_BYTES {
                return Err(MidnightRuntimeError::InvalidArgument);
            }
        }
        let trailing = cursor.length_prefixed()?;
        if !trailing.is_empty() {
            let update: MerkleTreeCollapsedUpdate = tagged_deserialize(&mut &trailing[..])
                .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
            shielded = shielded
                .apply_collapsed_update(&update)
                .map_err(|_| MidnightRuntimeError::SyncGap)?;
        }
        let _last_event_id = cursor.u64_le()?;
        if cursor.remaining() != 0 {
            return Err(MidnightRuntimeError::InvalidArgument);
        }

        let mut proposed = self.clone();
        proposed.shielded = shielded;
        proposed.refresh_coin_hashes(&keys)?;
        Ok(proposed)
    }

    pub(super) fn import_dust_v2(
        &self,
        payload: &[u8],
        dust_seed: &[u8],
    ) -> Result<Self, MidnightRuntimeError> {
        let mut cursor = BinaryCursor::new(payload);
        let parameter_bytes = cursor.length_prefixed()?;
        let parameters = if parameter_bytes.is_empty() {
            INITIAL_DUST_PARAMETERS
        } else {
            // The gateway serializes these tagged. A plain read consumes the
            // ASCII tag as field data and *succeeds*, yielding a ratio and decay
            // rate that are silently wrong — every coin then caps out at a
            // fraction of its real value instead of failing loudly.
            deserialize_tagged_or_plain::<DustParameters>(parameter_bytes)?
        };
        let sync_time = cursor.u64_le()?;
        let body_length = cursor
            .remaining()
            .checked_sub(8)
            .ok_or(MidnightRuntimeError::InvalidArgument)?;
        let body = cursor.take(body_length)?;
        let _last_event_id = cursor.u64_le()?;
        if cursor.remaining() != 0 {
            return Err(MidnightRuntimeError::InvalidArgument);
        }

        let mut seed: [u8; 32] = dust_seed
            .try_into()
            .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
        let secret_key = DustSecretKey::derive_secret_key(&seed);
        seed.zeroize();
        let public_key = DustPublicKey::from(secret_key.clone());
        let mut dust = DustLocalState::new(parameters);
        let mut body_cursor = BinaryCursor::new(body);

        let generation_count = usize::try_from(body_cursor.u32_le()?)
            .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
        if generation_count > MAX_SYNC_ENTRIES {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        for _ in 0..generation_count {
            let collapsed = body_cursor.length_prefixed()?;
            if !collapsed.is_empty() {
                let update: MerkleTreeCollapsedUpdate = tagged_deserialize(&mut &collapsed[..])
                    .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
                dust = dust
                    .apply_generation_collapsed_update(&update)
                    .map_err(|_| MidnightRuntimeError::SyncGap)?;
            }
            let info: DustGenerationInfo =
                deserialize_tagged_or_plain(body_cursor.length_prefixed()?)?;
            let index = body_cursor.u64_le()?;
            let own = (info.owner == public_key).then_some(info.nonce);
            dust = dust
                .insert_generation_info(index, info, own)
                .map_err(|_| MidnightRuntimeError::SyncGap)?;
        }
        let generation_trailing = body_cursor.length_prefixed()?;
        if !generation_trailing.is_empty() {
            let update: MerkleTreeCollapsedUpdate =
                tagged_deserialize(&mut &generation_trailing[..])
                    .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
            dust = dust
                .apply_generation_collapsed_update(&update)
                .map_err(|_| MidnightRuntimeError::SyncGap)?;
        }

        let commitment_count = usize::try_from(body_cursor.u32_le()?)
            .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
        if commitment_count > MAX_SYNC_ENTRIES {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        for _ in 0..commitment_count {
            let collapsed = body_cursor.length_prefixed()?;
            if !collapsed.is_empty() {
                let update: MerkleTreeCollapsedUpdate = tagged_deserialize(&mut &collapsed[..])
                    .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
                dust = dust
                    .apply_commitment_collapsed_update(&update)
                    .map_err(|_| MidnightRuntimeError::SyncGap)?;
            }
            let mut utxo: QualifiedDustOutput =
                deserialize_tagged_or_plain(body_cursor.length_prefixed()?)?;
            let index = body_cursor.u64_le()?;
            if utxo.mt_index != index {
                return Err(MidnightRuntimeError::InvalidArgument);
            }
            let own = utxo.owner == public_key;
            dust = dust
                .insert_commitment(index, utxo, own)
                .map_err(|_| MidnightRuntimeError::SyncGap)?;
            if own {
                utxo.mt_index = index;
                dust = dust
                    .add_utxo(&utxo.nullifier(&secret_key), &utxo, None)
                    .map_err(|_| MidnightRuntimeError::SyncGap)?;
            }
        }
        let commitment_trailing = body_cursor.length_prefixed()?;
        if !commitment_trailing.is_empty() {
            let update: MerkleTreeCollapsedUpdate =
                tagged_deserialize(&mut &commitment_trailing[..])
                    .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
            dust = dust
                .apply_commitment_collapsed_update(&update)
                .map_err(|_| MidnightRuntimeError::SyncGap)?;
        }
        if body_cursor.remaining() != 0 {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        dust.sync_time = midnight_base_crypto::time::Timestamp::from_secs(sync_time);

        // A snapshot can arrive with commitment and generating trees that no
        // longer match the chain, and nothing on the way in looks at them: the
        // divergence only surfaces at the node, as InvalidDustSpendProof, after
        // the wallet has already spent half a minute proving against it. Trial
        // spend every output at a zero fee — that resolves both merkle paths
        // while leaving the value check unreachable — and refuse the whole
        // import if any path fails to resolve, so the caller retries against a
        // fresh snapshot instead of building on a bad one.
        //
        // TTLs are processed first because that is what drops dead and orphaned
        // outputs; without it a single expired UTXO would reject every
        // otherwise sound snapshot.
        let now = midnight_base_crypto::time::Timestamp::from_secs(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );
        dust = dust.process_ttls(now);
        for utxo in dust.utxos().collect::<Vec<_>>() {
            dust.spend(&secret_key, &utxo, 0, now)
                .map_err(|_| MidnightRuntimeError::SyncGap)?;
        }

        let mut proposed = self.clone();
        proposed.dust = dust;
        Ok(proposed)
    }
}
