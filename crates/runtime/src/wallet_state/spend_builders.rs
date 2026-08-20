use super::*;

impl NativeWalletState {
    pub(super) fn build_dust_registration_with_rng<R: Rng + CryptoRng>(
        &self,
        network_id: &str,
        night_external_key: &[u8],
        dust_seed: &[u8],
        current_time_seconds: u64,
        ttl_seconds: u64,
        rng: &mut R,
    ) -> Result<(Self, Vec<u8>), MidnightRuntimeError> {
        if network_id.is_empty() || ttl_seconds <= current_time_seconds {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        let signing_key = SigningKey::from_bytes(night_external_key)
            .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
        let dust_seed: [u8; 32] = dust_seed
            .try_into()
            .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
        let dust_address = DustPublicKey::from(DustSecretKey::derive_secret_key(&dust_seed));
        let owner = UserAddress::from(signing_key.verifying_key());
        let night_token = UnshieldedTokenType(HashOutput::default());

        let generated_now = |coin: &UnshieldedUtxoWithMeta| -> Result<u128, MidnightRuntimeError> {
            let value = coin
                .utxo
                .value
                .parse::<u128>()
                .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
            let created_millis = u64::try_from(coin.meta.ctime.max(0))
                .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
            // Round creation up so the client never claims the partial first
            // second that the ledger excludes from DUST generation.
            let created_seconds = created_millis.saturating_add(999) / 1_000;
            let elapsed = current_time_seconds.saturating_sub(created_seconds) as u128;
            let cap = value.saturating_mul(self.dust.params.night_dust_ratio as u128);
            Ok(elapsed
                .saturating_mul(value)
                .saturating_mul(self.dust.params.generation_decay_rate as u128)
                .min(cap))
        };
        let mut selected = self
            .unshielded
            .available_utxos
            .iter()
            .filter(|coin| {
                !coin.meta.registered_for_dust_generation
                    && decode_hash(&coin.utxo.type_).ok() == Some(HashOutput::default())
            })
            .map(|coin| generated_now(coin).map(|generated| (generated, coin.clone())))
            .collect::<Result<Vec<_>, _>>()?;
        selected.sort_by_key(|entry| std::cmp::Reverse(entry.0));
        if selected.is_empty() {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        let guaranteed = vec![selected[0].1.clone()];
        let fallible = selected
            .iter()
            .skip(1)
            .map(|(_, coin)| coin.clone())
            .collect::<Vec<_>>();
        let allow_fee_payment = selected[0].0;

        let make_offer = |coins: &[UnshieldedUtxoWithMeta]| {
            if coins.is_empty() {
                return Ok(None);
            }
            let mut value = 0_u128;
            let mut inputs = Vec::with_capacity(coins.len());
            for coin in coins {
                let coin_value = coin
                    .utxo
                    .value
                    .parse::<u128>()
                    .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
                value = value
                    .checked_add(coin_value)
                    .ok_or(MidnightRuntimeError::InvalidArgument)?;
                inputs.push(UtxoSpend {
                    value: coin_value,
                    owner: signing_key.verifying_key(),
                    type_: UnshieldedTokenType(decode_hash(&coin.utxo.type_)?),
                    intent_hash: IntentHash(decode_hash(&coin.utxo.intent_hash)?),
                    output_no: coin.utxo.output_no,
                });
            }
            inputs.sort();
            Ok(Some(UnshieldedOffer::<Signature, InMemoryDB> {
                inputs: inputs.into_iter().collect(),
                outputs: vec![UtxoOutput {
                    value,
                    owner,
                    type_: night_token,
                }]
                .into_iter()
                .collect(),
                signatures: Vec::new().into_iter().collect(),
            }))
        };
        let guaranteed_offer = make_offer(&guaranteed)?;
        let fallible_offer = make_offer(&fallible)?;
        let registration = DustRegistration::<Signature, InMemoryDB> {
            night_key: signing_key.verifying_key(),
            dust_address: Some(Sp::new(dust_address)),
            allow_fee_payment,
            signature: None,
        };
        let dust_actions = DustActions::<Signature, ProofPreimageMarker, InMemoryDB> {
            spends: Vec::new().into_iter().collect(),
            registrations: vec![registration].into_iter().collect(),
            ctime: midnight_base_crypto::time::Timestamp::from_secs(current_time_seconds),
        };
        let mut intent =
            Intent::<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB>::new(
                rng,
                guaranteed_offer,
                fallible_offer,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Some(dust_actions),
                midnight_base_crypto::time::Timestamp::from_secs(ttl_seconds),
            );
        let data_to_sign = intent.erase_proofs().erase_signatures().data_to_sign(1);
        let signature = signing_key.sign(rng, &data_to_sign);
        for offer in [
            &mut intent.guaranteed_unshielded_offer,
            &mut intent.fallible_unshielded_offer,
        ] {
            if let Some(existing) = offer.as_ref() {
                let mut signed = (**existing).clone();
                signed.add_signatures(vec![signature.clone(); signed.inputs.len()]);
                *offer = Some(Sp::new(signed));
            }
        }
        let actions = intent
            .dust_actions
            .as_ref()
            .ok_or(MidnightRuntimeError::NativeInternal)?;
        let mut signed_actions = (**actions).clone();
        let mut signed_registration = signed_actions
            .registrations
            .iter_deref()
            .next()
            .cloned()
            .ok_or(MidnightRuntimeError::NativeInternal)?;
        signed_registration.signature = Some(Sp::new(signature));
        signed_actions.registrations = vec![signed_registration].into_iter().collect();
        intent.dust_actions = Some(Sp::new(signed_actions));

        let transaction =
            Transaction::<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB>::new(
                network_id.to_owned(),
                [(1_u16, intent)].into_iter().collect(),
                None,
                LedgerHashMap::new(),
            );
        let mut raw = Vec::new();
        tagged_serialize(&transaction, &mut raw)
            .map_err(|_| MidnightRuntimeError::NativeInternal)?;

        let mut proposed = self.clone();
        for (_, coin) in selected {
            remove_utxo(&mut proposed.unshielded.available_utxos, &coin.utxo);
            upsert_utxo(&mut proposed.unshielded.pending_utxos, coin);
        }
        Ok((proposed, raw))
    }

    pub(super) fn build_unshielded_transfer_with_rng<R: Rng + CryptoRng>(
        &self,
        input: UnshieldedTransferInput<'_>,
        rng: &mut R,
    ) -> Result<(Self, Vec<u8>), MidnightRuntimeError> {
        let UnshieldedTransferInput {
            network_id,
            target_address,
            amount,
            token_type,
            night_external_key,
            ttl_seconds,
        } = input;
        if amount == 0 || network_id.is_empty() {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        let target = decode_unshielded_address(target_address, network_id)?;
        let token_type = UnshieldedTokenType(decode_hash(token_type)?);
        let signing_key = SigningKey::from_bytes(night_external_key)
            .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
        let owner = UserAddress::from(signing_key.verifying_key());

        let mut available = self
            .unshielded
            .available_utxos
            .iter()
            .filter_map(|coin| {
                let candidate_type = decode_hash(&coin.utxo.type_).ok()?;
                (candidate_type == token_type.0).then_some(coin.clone())
            })
            .collect::<Vec<_>>();
        available.sort_by_key(|coin| coin.utxo.value.parse::<u128>().unwrap_or(u128::MAX));

        let mut selected = Vec::new();
        let mut selected_value = 0_u128;
        for coin in available {
            let value = coin
                .utxo
                .value
                .parse::<u128>()
                .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
            selected_value = selected_value
                .checked_add(value)
                .ok_or(MidnightRuntimeError::InvalidArgument)?;
            selected.push(coin);
            if selected_value >= amount {
                break;
            }
        }
        if selected_value < amount {
            return Err(MidnightRuntimeError::InvalidArgument);
        }

        let mut inputs = selected
            .iter()
            .map(|coin| {
                Ok(UtxoSpend {
                    value: coin
                        .utxo
                        .value
                        .parse::<u128>()
                        .map_err(|_| MidnightRuntimeError::StateIncompatible)?,
                    owner: signing_key.verifying_key(),
                    type_: UnshieldedTokenType(decode_hash(&coin.utxo.type_)?),
                    intent_hash: IntentHash(decode_hash(&coin.utxo.intent_hash)?),
                    output_no: coin.utxo.output_no,
                })
            })
            .collect::<Result<Vec<_>, MidnightRuntimeError>>()?;
        inputs.sort();
        let mut outputs = vec![UtxoOutput {
            value: amount,
            owner: target,
            type_: token_type,
        }];
        let change = selected_value - amount;
        if change > 0 {
            outputs.push(UtxoOutput {
                value: change,
                owner,
                type_: token_type,
            });
        }
        outputs.sort();

        let unsigned_offer = UnshieldedOffer::<Signature, InMemoryDB> {
            inputs: inputs.clone().into_iter().collect(),
            outputs: outputs.into_iter().collect(),
            signatures: Vec::new().into_iter().collect(),
        };
        let is_night = token_type.0 == HashOutput::default();
        let mut intent =
            Intent::<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB>::new(
                rng,
                (!is_night).then_some(unsigned_offer.clone()),
                is_night.then_some(unsigned_offer),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                midnight_base_crypto::time::Timestamp::from_secs(ttl_seconds),
            );
        let data_to_sign = intent.erase_proofs().erase_signatures().data_to_sign(1);
        let signature = signing_key.sign(rng, &data_to_sign);
        let target_offer = if is_night {
            &mut intent.fallible_unshielded_offer
        } else {
            &mut intent.guaranteed_unshielded_offer
        };
        let offer = target_offer
            .as_ref()
            .ok_or(MidnightRuntimeError::NativeInternal)?;
        let mut signed_offer = (**offer).clone();
        signed_offer.add_signatures(vec![signature; inputs.len()]);
        *target_offer = Some(Sp::new(signed_offer));

        let transaction =
            Transaction::<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB>::new(
                network_id.to_owned(),
                [(1_u16, intent)].into_iter().collect(),
                None,
                LedgerHashMap::new(),
            );
        let mut raw = Vec::new();
        tagged_serialize(&transaction, &mut raw)
            .map_err(|_| MidnightRuntimeError::NativeInternal)?;

        let mut proposed = self.clone();
        for coin in selected {
            remove_utxo(&mut proposed.unshielded.available_utxos, &coin.utxo);
            upsert_utxo(&mut proposed.unshielded.pending_utxos, coin);
        }
        Ok((proposed, raw))
    }
}
