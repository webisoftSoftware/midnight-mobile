impl NativeWalletState {
    fn build_shielded_transfer_with_rng<R: Rng + CryptoRng + ?Sized>(
        &self,
        network_id: &str,
        target_address: &str,
        amount: u128,
        token_type: &str,
        zswap_seed: &[u8],
        rng: &mut R,
    ) -> Result<(Self, Vec<u8>), MidnightRuntimeError> {
        if amount == 0 || network_id.is_empty() {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        let token_type = decode_hash(token_type).map(ShieldedTokenType)?;
        let (target_cpk, target_epk) = decode_shielded_address(target_address, network_id)?;
        let seed: [u8; 32] = zswap_seed
            .try_into()
            .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
        let secret_keys = ZswapSecretKeys::from(ZswapSeed::from(seed));

        // Match the pinned SDK's RNG order: desired output first, then selected
        // inputs, then the self-addressed change output.
        let target_coin = ShieldedCoinInfo::new(rng, amount, token_type);
        let target_output = ZswapOutput::<ProofPreimage, InMemoryDB>::new(
            rng,
            &target_coin,
            Some(0),
            &target_cpk,
            Some(target_epk),
        )
        .map_err(|_| MidnightRuntimeError::NativeInternal)?;

        let mut available = self
            .shielded
            .coins
            .iter()
            .filter(|(nullifier, coin)| {
                coin.type_ == token_type && !self.shielded.pending_spends.contains_key(nullifier)
            })
            .map(|(_, coin)| *coin)
            .collect::<Vec<_>>();
        available.sort_by_key(|coin| coin.value);

        let mut proposed = self.clone();
        let mut inputs = Vec::new();
        let mut selected_value = 0_u128;
        for coin in available {
            let (next, input) = proposed
                .shielded
                .spend(rng, &secret_keys, &coin, Some(0))
                .map_err(|_| MidnightRuntimeError::NativeInternal)?;
            proposed.shielded = next;
            inputs.push(input);
            selected_value = selected_value
                .checked_add(coin.value)
                .ok_or(MidnightRuntimeError::InvalidArgument)?;
            if selected_value >= amount {
                break;
            }
        }
        if selected_value < amount {
            return Err(MidnightRuntimeError::InvalidArgument);
        }

        let mut outputs = vec![target_output];
        let change = selected_value - amount;
        if change > 0 {
            let change_coin = ShieldedCoinInfo::new(rng, change, token_type);
            outputs.push(
                ZswapOutput::<ProofPreimage, InMemoryDB>::new(
                    rng,
                    &change_coin,
                    Some(0),
                    &secret_keys.coin_public_key(),
                    Some(secret_keys.enc_public_key()),
                )
                .map_err(|_| MidnightRuntimeError::NativeInternal)?,
            );
            proposed.shielded = proposed
                .shielded
                .watch_for(&secret_keys.coin_public_key(), &change_coin);
        }
        if target_cpk == secret_keys.coin_public_key() {
            proposed.shielded = proposed
                .shielded
                .watch_for(&secret_keys.coin_public_key(), &target_coin);
        }

        let offer =
            ZswapOffer::new(inputs, outputs, Vec::new()).ok_or(MidnightRuntimeError::NativeInternal)?;
        let transaction =
            Transaction::<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB>::new(
                network_id.to_owned(),
                LedgerHashMap::new(),
                Some(offer),
                LedgerHashMap::new(),
            );
        let mut raw = Vec::new();
        tagged_serialize(&transaction, &mut raw).map_err(|_| MidnightRuntimeError::NativeInternal)?;
        proposed.refresh_coin_hashes(&secret_keys)?;
        Ok((proposed, raw))
    }

    fn validate_coin_hash_keys(&self) -> Result<(), MidnightRuntimeError> {
        for (_, coin) in self.shielded.coins.iter() {
            let nonce = serializable_hex(&coin.nonce)?;
            if !self.coin_hashes.contains_key(&nonce) {
                return Err(MidnightRuntimeError::StateIncompatible);
            }
        }
        for (_, coin) in self.shielded.pending_outputs.iter() {
            let nonce = serializable_hex(&coin.nonce)?;
            if !self.coin_hashes.contains_key(&nonce) {
                return Err(MidnightRuntimeError::StateIncompatible);
            }
        }
        Ok(())
    }

    fn refresh_coin_hashes(
        &mut self,
        secret_keys: &ZswapSecretKeys,
    ) -> Result<(), MidnightRuntimeError> {
        let mut current = BTreeMap::new();
        for (_, coin) in self.shielded.coins.iter() {
            self.insert_coin_hash(&mut current, secret_keys, &ShieldedCoinInfo::from(&*coin))?;
        }
        for (_, coin) in self.shielded.pending_outputs.iter() {
            self.insert_coin_hash(&mut current, secret_keys, &coin)?;
        }
        self.coin_hashes = current;
        Ok(())
    }

    fn insert_coin_hash(
        &self,
        destination: &mut BTreeMap<String, CoinHashes>,
        secret_keys: &ZswapSecretKeys,
        coin: &ShieldedCoinInfo,
    ) -> Result<(), MidnightRuntimeError> {
        let nonce = serializable_hex(&coin.nonce)?;
        if let Some(existing) = self.coin_hashes.get(&nonce) {
            destination.insert(nonce, existing.clone());
            return Ok(());
        }
        let commitment = coin.commitment(&Recipient::User(secret_keys.coin_public_key()));
        let nullifier = coin.nullifier(&SenderEvidence::User(Cow::Borrowed(
            &secret_keys.coin_secret_key,
        )));
        destination.insert(
            nonce,
            CoinHashes {
                commitment: serializable_hex(&commitment)?,
                nullifier: serializable_hex(&nullifier)?,
            },
        );
        Ok(())
    }

    fn apply_unshielded(&mut self, payload: &[u8]) -> Result<(), MidnightRuntimeError> {
        let update: UnshieldedSyncUpdate =
            serde_json::from_slice(payload).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
        match update {
            UnshieldedSyncUpdate::UnshieldedTransactionsProgress {
                highest_transaction_id,
            } => {
                let _ = highest_transaction_id;
            }
            UnshieldedSyncUpdate::UnshieldedTransaction {
                transaction,
                created_utxos,
                spent_utxos,
            } => {
                let status = if transaction.type_ == "SystemTransaction" {
                    "SUCCESS"
                } else {
                    transaction
                        .transaction_result
                        .as_ref()
                        .map(|result| result.status.as_str())
                        .ok_or(MidnightRuntimeError::InvalidArgument)?
                };
                if !matches!(status, "SUCCESS" | "PARTIAL_SUCCESS" | "FAILURE") {
                    return Err(MidnightRuntimeError::InvalidArgument);
                }
                let transaction_timestamp_ms =
                    transaction.block.as_ref().map(|block| block.timestamp);
                let spent = spent_utxos
                    .into_iter()
                    .map(|utxo| utxo.into_legacy(transaction_timestamp_ms))
                    .collect::<Result<Vec<_>, _>>()?;
                let created = created_utxos
                    .into_iter()
                    .map(|utxo| utxo.into_legacy(transaction_timestamp_ms))
                    .collect::<Result<Vec<_>, _>>()?;
                if status == "FAILURE" {
                    for utxo in spent {
                        remove_utxo(&mut self.unshielded.pending_utxos, &utxo.utxo);
                        upsert_utxo(&mut self.unshielded.available_utxos, utxo);
                    }
                } else {
                    for utxo in spent {
                        remove_utxo(&mut self.unshielded.available_utxos, &utxo.utxo);
                        remove_utxo(&mut self.unshielded.pending_utxos, &utxo.utxo);
                    }
                    for utxo in created {
                        upsert_utxo(&mut self.unshielded.available_utxos, utxo);
                    }
                }
                let _ = transaction.id;
            }
        }
        Ok(())
    }

    pub(crate) fn export_legacy(
        &self,
        context: LegacyExportContext<'_>,
    ) -> Result<LegacyWalletState, MidnightRuntimeError> {
        let LegacyExportContext {
            network_id,
            unshielded_address,
            address_material,
            night_external_key,
            shielded_offset,
            dust_offset,
            unshielded_offset,
        } = context;
        let signing_key = SigningKey::from_bytes(night_external_key)
            .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
        let shielded = ShieldedSnapshot {
            public_keys: ShieldedPublicKeys {
                coin_public_key: address_material.shielded_coin_public_key_hex.clone(),
                encryption_public_key: address_material.shielded_encryption_public_key_hex.clone(),
            },
            state: encode_hex_state(&self.shielded)?,
            protocol_version: self.protocol_version.clone(),
            offset: shielded_offset.map(|value| value.to_string()),
            network_id: network_id.to_owned(),
            coin_hashes: self.coin_hashes.clone(),
        };
        let dust = DustSnapshot {
            public_key: DustPublicKeySnapshot {
                public_key: address_material.dust_public_key.clone(),
            },
            state: encode_hex_state(&self.dust)?,
            protocol_version: self.protocol_version.clone(),
            network_id: network_id.to_owned(),
            offset: dust_offset.map(|value| value.to_string()),
        };
        let unshielded = UnshieldedSnapshot {
            public_key: UnshieldedPublicKey {
                public_key: serializable_hex(&signing_key.verifying_key())?,
                address_hex: address_material.unshielded_address_hex.clone(),
                address: unshielded_address.to_owned(),
            },
            state: self.unshielded.clone(),
            protocol_version: self.protocol_version.clone(),
            applied_id: unshielded_offset.map(|value| value.to_string()),
            network_id: network_id.to_owned(),
        };
        Ok(LegacyWalletState {
            serialized_dust_wallet_state: to_snapshot_json(&dust)?,
            serialized_shielded_wallet_state: to_snapshot_json(&shielded)?,
            serialized_unshielded_wallet_state: to_snapshot_json(&unshielded)?,
        })
    }
}
