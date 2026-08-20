use super::*;

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
                    let raw = hex::decode(&wire.raw)
                        .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
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
            "shielded-v2" => {
                if payloads.len() != 1 {
                    return Err(MidnightRuntimeError::InvalidArgument);
                }
                proposed = self.import_shielded_v2(&payloads[0], zswap_seed)?;
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
            "dust-v2" => {
                if payloads.len() != 1 {
                    return Err(MidnightRuntimeError::InvalidArgument);
                }
                proposed = self.import_dust_v2(&payloads[0], dust_seed)?;
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

    pub(crate) fn shielded_mint_context(
        &self,
        zswap_seed: &[u8],
    ) -> Result<ShieldedMintContext, MidnightRuntimeError> {
        let mut seed: [u8; 32] = zswap_seed
            .try_into()
            .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
        let keys = ZswapSecretKeys::from(ZswapSeed::from(seed));
        seed.zeroize();
        Ok(ShieldedMintContext {
            coin_public_key: keys.coin_public_key(),
            encryption_public_key: keys.enc_public_key(),
            output_index: self.shielded.first_free,
        })
    }

    pub(crate) fn watch_shielded_mint(
        &self,
        raw_coin: &[u8],
        expected_output_index: u64,
        zswap_seed: &[u8],
    ) -> Result<(Self, u64), MidnightRuntimeError> {
        if self.shielded.first_free != expected_output_index {
            return Err(MidnightRuntimeError::SyncGap);
        }
        let mut remaining = raw_coin;
        let coin: ShieldedCoinInfo = tagged_deserialize(&mut remaining)
            .map_err(|_| MidnightRuntimeError::InvalidArgument)?;
        let mut canonical = Vec::new();
        tagged_serialize(&coin, &mut canonical)
            .map_err(|_| MidnightRuntimeError::NativeInternal)?;
        if !remaining.is_empty() || canonical != raw_coin {
            return Err(MidnightRuntimeError::InvalidArgument);
        }

        let mut seed: [u8; 32] = zswap_seed
            .try_into()
            .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
        let keys = ZswapSecretKeys::from(ZswapSeed::from(seed));
        seed.zeroize();
        let mut proposed = self.clone();
        proposed.shielded = proposed.shielded.watch_for(&keys.coin_public_key(), &coin);
        proposed.refresh_coin_hashes(&keys)?;
        let output_index = proposed.shielded.first_free;
        Ok((proposed, output_index))
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
            Transaction::Standard(standard) => standard.intents.get(&1).and_then(|intent| {
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
}
