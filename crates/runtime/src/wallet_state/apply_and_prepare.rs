impl NativeWalletState {
    pub(crate) fn apply_batch(
        &self,
        stream: &str,
        payloads: &[Vec<u8>],
        zswap_seed: &[u8],
        dust_seed: &[u8],
    ) -> Result<Self, MidnightRuntimeError> {
        let mut proposed = self.clone();
        match stream {
            "shielded" => {
                let mut seed: [u8; 32] = zswap_seed
                    .try_into()
                    .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
                let keys = ZswapSecretKeys::from(ZswapSeed::from(seed));
                seed.zeroize();
                let events = decode_event_payloads(payloads)?;
                proposed.shielded = proposed
                    .shielded
                    .replay_events(&keys, events.iter())
                    .map_err(|_| MidnightRuntimeError::SyncGap)?;
                proposed.refresh_coin_hashes(&keys)?;
            }
            "shielded-wire" => {
                let mut seed: [u8; 32] = zswap_seed
                    .try_into()
                    .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
                let keys = ZswapSecretKeys::from(ZswapSeed::from(seed));
                seed.zeroize();
                let mut events = Vec::new();
                let mut protocol_version = None;
                for payload in payloads {
                    let wire: ShieldedWireEvent = serde_json::from_slice(payload)
                        .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
                    let _ = wire.id;
                    if wire.raw.is_empty()
                        || !wire.raw.len().is_multiple_of(2)
                        || !wire.raw.bytes().all(|byte| byte.is_ascii_hexdigit())
                    {
                        return Err(MidnightRuntimeError::InvalidArgument);
                    }
                    let raw =
                        hex::decode(&wire.raw).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
                    events.extend(
                        tagged_deserialize_sequence::<Event<InMemoryDB>>(&raw[..])
                            .map_err(|_| MidnightRuntimeError::InvalidArgument)?,
                    );
                    match protocol_version {
                        Some(version) if version != wire.protocol_version => {
                            return Err(MidnightRuntimeError::InvalidArgument);
                        }
                        None => protocol_version = Some(wire.protocol_version),
                        Some(_) => {}
                    }
                }
                proposed.shielded = proposed
                    .shielded
                    .replay_events(&keys, events.iter())
                    .map_err(|_| MidnightRuntimeError::SyncGap)?;
                proposed.refresh_coin_hashes(&keys)?;
                if let Some(protocol_version) = protocol_version {
                    proposed.protocol_version = protocol_version.to_string();
                }
            }
            "dust-wire" => {
                let mut raw_payloads = Vec::with_capacity(payloads.len());
                for payload in payloads {
                    let wire: DustWireEvent = serde_json::from_slice(payload)
                        .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
                    let _ = wire.id;
                    if wire.raw.is_empty()
                        || !wire.raw.len().is_multiple_of(2)
                        || !wire.raw.bytes().all(|byte| byte.is_ascii_hexdigit())
                    {
                        return Err(MidnightRuntimeError::InvalidArgument);
                    }
                    raw_payloads.push(
                        hex::decode(wire.raw).map_err(|_| MidnightRuntimeError::InvalidArgument)?,
                    );
                }
                let mut seed: [u8; 32] = dust_seed
                    .try_into()
                    .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
                let key = DustSecretKey::derive_secret_key(&seed);
                seed.zeroize();
                let events = decode_event_payloads(&raw_payloads)?;
                proposed.dust = proposed
                    .dust
                    .replay_events(&key, events.iter())
                    .map_err(|_| MidnightRuntimeError::SyncGap)?;
            }
            "dust" => {
                let mut seed: [u8; 32] = dust_seed
                    .try_into()
                    .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
                let key = DustSecretKey::derive_secret_key(&seed);
                seed.zeroize();
                let events = decode_event_payloads(payloads)?;
                proposed.dust = proposed
                    .dust
                    .replay_events(&key, events.iter())
                    .map_err(|_| MidnightRuntimeError::SyncGap)?;
            }
            "unshielded" => {
                for payload in payloads {
                    proposed.apply_unshielded(payload)?;
                }
            }
            _ => return Err(MidnightRuntimeError::InvalidArgument),
        }
        Ok(proposed)
    }

    pub(crate) fn build_shielded_transfer(
        &self,
        network_id: &str,
        target_address: &str,
        amount: u128,
        token_type: &str,
        zswap_seed: &[u8],
    ) -> Result<(Self, Vec<u8>), MidnightRuntimeError> {
        self.build_shielded_transfer_with_rng(
            network_id,
            target_address,
            amount,
            token_type,
            zswap_seed,
            &mut OsRng,
        )
    }

    pub(crate) fn build_unshielded_transfer(
        &self,
        network_id: &str,
        target_address: &str,
        amount: u128,
        token_type: &str,
        night_external_key: &[u8],
        ttl_seconds: u64,
    ) -> Result<(Self, Vec<u8>), MidnightRuntimeError> {
        self.build_unshielded_transfer_with_rng(
            UnshieldedTransferInput {
                network_id,
                target_address,
                amount,
                token_type,
                night_external_key,
                ttl_seconds,
            },
            &mut OsRng,
        )
    }

    pub(crate) fn build_dust_registration(
        &self,
        network_id: &str,
        night_external_key: &[u8],
        dust_seed: &[u8],
        current_time_seconds: u64,
        ttl_seconds: u64,
    ) -> Result<(Self, Vec<u8>), MidnightRuntimeError> {
        self.build_dust_registration_with_rng(
            network_id,
            night_external_key,
            dust_seed,
            current_time_seconds,
            ttl_seconds,
            &mut OsRng,
        )
    }

    pub(crate) fn validate_dust_registration_fee(
        &self,
        raw: &[u8],
        ledger_parameters: &LedgerParameters,
        fee_blocks_margin: usize,
        additional_fee_overhead: u128,
    ) -> Result<(), MidnightRuntimeError> {
        if self.dust.params != ledger_parameters.dust {
            return Err(MidnightRuntimeError::StateIncompatible);
        }
        let transaction: Transaction<
            Signature,
            ProofPreimageMarker,
            PedersenRandomness,
            InMemoryDB,
        > = tagged_deserialize(&mut &raw[..]).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
        let allow_fee_payment = match &transaction {
            Transaction::Standard(standard) => standard
                .intents
                .get(&1)
                .and_then(|intent| {
                    intent.dust_actions.as_ref().and_then(|actions| {
                        actions
                            .registrations
                            .iter()
                            .next()
                            .map(|registration| registration.allow_fee_payment)
                    })
                }),
            Transaction::ClaimRewards(_) => None,
        }
        .ok_or(MidnightRuntimeError::InvalidArgument)?;
        let required_fee = transaction
            .fees_with_margin(ledger_parameters, fee_blocks_margin)
            .map_err(|_| MidnightRuntimeError::InvalidArgument)?
            .checked_add(additional_fee_overhead)
            .ok_or(MidnightRuntimeError::InvalidArgument)?;
        if allow_fee_payment < required_fee {
            return Err(MidnightRuntimeError::InsufficientDust);
        }
        Ok(())
    }

    /// Build the fee-paying counterpart for an already signed transaction.
    ///
    /// The input is proof-erased so this path is shared by pre-binding and
    /// finalized dApp transactions. The returned transaction contains only
    /// the locally selected DUST spends; the caller proves it remotely and
    /// merges it back into the original transaction. State is proposed only:
    /// selected DUST outputs become pending only in the returned clone. A
    /// balance-only preparation path discards that clone so an open dApp
    /// transaction does not reduce the confirmed wallet snapshot.
    pub(crate) fn build_dust_balance(
        &self,
        input: DustBalanceInput<'_>,
    ) -> Result<(Self, Option<Vec<u8>>), MidnightRuntimeError> {
        let DustBalanceInput {
            network_id,
            original,
            ledger_parameters,
            fee_blocks_margin,
            additional_fee_overhead,
            dust_seed,
            current_time_seconds,
            ttl_seconds,
        } = input;
        if network_id.is_empty()
            || ttl_seconds <= current_time_seconds
            || self.dust.params != ledger_parameters.dust
        {
            return Err(MidnightRuntimeError::StateIncompatible);
        }
        let original_network = match original {
            Transaction::Standard(transaction) => transaction.network_id.as_str(),
            Transaction::ClaimRewards(_) => return Err(MidnightRuntimeError::InvalidArgument),
        };
        if original_network != network_id {
            return Err(MidnightRuntimeError::InvalidArgument);
        }

        let segment = match original {
            Transaction::Standard(transaction) => (1_u16..=u16::MAX)
                .find(|candidate| !transaction.intents.contains_key(candidate))
                .ok_or(MidnightRuntimeError::InvalidArgument)?,
            Transaction::ClaimRewards(_) => unreachable!(),
        };
        let mut dust_seed: [u8; 32] = dust_seed
            .try_into()
            .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
        let dust_key = DustSecretKey::derive_secret_key(&dust_seed);
        dust_seed.zeroize();
        let current_time = midnight_base_crypto::time::Timestamp::from_secs(current_time_seconds);
        let ttl = midnight_base_crypto::time::Timestamp::from_secs(ttl_seconds);

        let mut available = self
            .dust
            .utxos()
            .filter_map(|coin| {
                let generation = self.dust.generation_info(&coin)?;
                let value = midnight_ledger::dust::DustOutput::from(coin).updated_value(
                    &generation,
                    current_time,
                    &self.dust.params,
                );
                (value > 0).then_some((value, coin))
            })
            .collect::<Vec<_>>();
        available.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.nonce.as_le_bytes().cmp(&right.1.nonce.as_le_bytes()))
        });

        let fee_shortfall = |total_fee: u128| -> Result<u128, MidnightRuntimeError> {
            let balances = original
                .balance(Some(total_fee))
                .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
            for ((token, candidate_segment), value) in &balances {
                if (*token, *candidate_segment) != (TokenType::Dust, 0) && *value != 0 {
                    // This operation is deliberately local-DUST balancing. A
                    // non-DUST imbalance must be resolved by the wallet that
                    // owns that token, never hidden behind fee sponsorship.
                    return Err(MidnightRuntimeError::InvalidArgument);
                }
            }
            match balances.get(&(TokenType::Dust, 0)).copied().unwrap_or(0) {
                value if value < 0 => value
                    .checked_neg()
                    .and_then(|value| u128::try_from(value).ok())
                    .ok_or(MidnightRuntimeError::InvalidArgument),
                0 => Ok(0),
                _ => Err(MidnightRuntimeError::InvalidArgument),
            }
        };

        let build_balancing = |deductions: &[u128]| -> DustBalancingResult {
            let mut dust_state = self.dust.clone();
            let mut spends = Vec::with_capacity(deductions.len());
            for ((_, coin), deduction) in available.iter().zip(deductions) {
                let (next, spend) = dust_state
                    .spend(&dust_key, coin, *deduction, current_time)
                    .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
                dust_state = next;
                spends.push(spend);
            }
            let actions = DustActions::<Signature, ProofPreimageMarker, InMemoryDB> {
                spends: spends.into_iter().collect(),
                registrations: Vec::new().into_iter().collect(),
                ctime: current_time,
            };
            let intent =
                Intent::<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB>::new(
                    &mut OsRng,
                    None,
                    None,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Some(actions),
                    ttl,
                );
            Ok((
                dust_state,
                Transaction::<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB>::new(
                    network_id.to_owned(),
                    [(segment, intent)].into_iter().collect(),
                    None,
                    LedgerHashMap::new(),
                ),
            ))
        };

        let calculate_combined_fee = |balancing: &Transaction<
            Signature,
            ProofPreimageMarker,
            PedersenRandomness,
            InMemoryDB,
        >|
         -> Result<u128, MidnightRuntimeError> {
            let merged = original
                .merge(&balancing.erase_proofs())
                .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
            merged
                .fees_with_margin(ledger_parameters, fee_blocks_margin)
                .map_err(|_| MidnightRuntimeError::InvalidArgument)?
                .checked_add(additional_fee_overhead)
                .ok_or(MidnightRuntimeError::InvalidArgument)
        };

        // A no-DUST transaction is still charged for the balancing spend that
        // it requires. Select the smallest outputs first, then converge after
        // serializing the exact spend values because integer encoding size is
        // part of the fee model.
        let original_fee = original
            .fees_with_margin(ledger_parameters, fee_blocks_margin)
            .map_err(|_| MidnightRuntimeError::InvalidArgument)?
            .checked_add(additional_fee_overhead)
            .ok_or(MidnightRuntimeError::InvalidArgument)?;
        let mut target = fee_shortfall(original_fee)?;
        if target == 0 {
            return Ok((self.clone(), None));
        }

        let mut selected_count = 0_usize;
        let mut deductions = Vec::<u128>::new();
        let mut final_state = None;
        let mut final_transaction = None;
        for _ in 0..64 {
            while available
                .iter()
                .take(selected_count)
                .map(|(value, _)| *value)
                .fold(0_u128, u128::saturating_add)
                < target
            {
                if selected_count >= available.len() {
                    return Err(MidnightRuntimeError::InvalidArgument);
                }
                selected_count += 1;
            }

            deductions.clear();
            let mut remaining = target;
            for (value, _) in available.iter().take(selected_count) {
                let deduction = remaining.min(*value);
                deductions.push(deduction);
                remaining -= deduction;
            }
            if remaining != 0 {
                return Err(MidnightRuntimeError::InvalidArgument);
            }

            let (candidate_state, candidate_transaction) = build_balancing(&deductions)?;
            let exact_fee = calculate_combined_fee(&candidate_transaction)?;
            let exact_target = fee_shortfall(exact_fee)?;
            if exact_target == target {
                final_state = Some(candidate_state);
                final_transaction = Some(candidate_transaction);
                break;
            }
            target = exact_target;
        }
        let mut proposed = self.clone();
        proposed.dust = final_state.ok_or(MidnightRuntimeError::InsufficientDust)?;
        let transaction = final_transaction.ok_or(MidnightRuntimeError::InsufficientDust)?;
        let mut raw = Vec::new();
        tagged_serialize(&transaction, &mut raw).map_err(|_| MidnightRuntimeError::NativeInternal)?;
        Ok((proposed, Some(raw)))
    }
}
