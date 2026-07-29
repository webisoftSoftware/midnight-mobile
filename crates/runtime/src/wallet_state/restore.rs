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

}
