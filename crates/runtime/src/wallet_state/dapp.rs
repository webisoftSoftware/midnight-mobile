use super::*;

const DAPP_MAX_COMPONENTS: usize = 50;

#[derive(Clone, Debug)]
pub(crate) struct DappIntentInput {
    pub(crate) wallet_type: String,
    pub(crate) token_type: String,
    pub(crate) amount: u128,
}

#[derive(Clone, Debug)]
pub(crate) struct DappTransactionOutput {
    pub(crate) wallet_type: String,
    pub(crate) token_type: String,
    pub(crate) amount: u128,
    pub(crate) receiver_address: String,
}

pub(crate) struct DappIntentBuildInput<'a> {
    pub(crate) network_id: &'a str,
    pub(crate) inputs: &'a [DappIntentInput],
    pub(crate) outputs: &'a [DappTransactionOutput],
    pub(crate) night_external_key: &'a [u8],
    pub(crate) zswap_seed: &'a [u8],
    pub(crate) ttl_seconds: u64,
}

struct UnshieldedIntentInput<'a> {
    network_id: &'a str,
    outputs: &'a [DappTransactionOutput],
    targets: Vec<(UnshieldedTokenType, u128)>,
    night_external_key: &'a [u8],
    ttl_seconds: u64,
    fallible: bool,
}

fn add_target<T: Copy + Eq>(
    targets: &mut Vec<(T, u128)>,
    token_type: T,
    amount: u128,
) -> Result<(), MidnightRuntimeError> {
    if amount == 0 {
        return Err(MidnightRuntimeError::InvalidArgument);
    }
    if let Some((_, total)) = targets
        .iter_mut()
        .find(|(candidate, _)| *candidate == token_type)
    {
        *total = total
            .checked_add(amount)
            .ok_or(MidnightRuntimeError::InvalidArgument)?;
    } else {
        targets.push((token_type, amount));
    }
    Ok(())
}

fn shielded_targets(
    values: impl Iterator<Item = (String, u128)>,
) -> Result<Vec<(ShieldedTokenType, u128)>, MidnightRuntimeError> {
    let mut targets = Vec::new();
    for (token_type, amount) in values {
        add_target(
            &mut targets,
            ShieldedTokenType(decode_hash(&token_type)?),
            amount,
        )?;
    }
    Ok(targets)
}

fn unshielded_targets(
    values: impl Iterator<Item = (String, u128)>,
) -> Result<Vec<(UnshieldedTokenType, u128)>, MidnightRuntimeError> {
    let mut targets = Vec::new();
    for (token_type, amount) in values {
        add_target(
            &mut targets,
            UnshieldedTokenType(decode_hash(&token_type)?),
            amount,
        )?;
    }
    Ok(targets)
}

fn build_shielded_offer<R: Rng + CryptoRng + ?Sized>(
    proposed: &mut NativeWalletState,
    network_id: &str,
    outputs: &[DappTransactionOutput],
    targets: Vec<(ShieldedTokenType, u128)>,
    zswap_seed: &[u8],
    rng: &mut R,
) -> Result<Option<ZswapOffer<ProofPreimage, InMemoryDB>>, MidnightRuntimeError> {
    if outputs.is_empty() && targets.is_empty() {
        return Ok(None);
    }
    let mut seed: [u8; 32] = zswap_seed
        .try_into()
        .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
    let secret_keys = ZswapSecretKeys::from(ZswapSeed::from(seed));
    seed.zeroize();

    let mut ledger_outputs = Vec::new();
    for output in outputs {
        let token_type = ShieldedTokenType(decode_hash(&output.token_type)?);
        let (target_cpk, target_epk) =
            decode_shielded_address(&output.receiver_address, network_id)?;
        let coin = ShieldedCoinInfo::new(rng, output.amount, token_type);
        ledger_outputs.push(
            ZswapOutput::<ProofPreimage, InMemoryDB>::new(
                rng,
                &coin,
                Some(0),
                &target_cpk,
                Some(target_epk),
            )
            .map_err(|_| MidnightRuntimeError::NativeInternal)?,
        );
        if target_cpk == secret_keys.coin_public_key() {
            proposed.shielded = proposed
                .shielded
                .watch_for(&secret_keys.coin_public_key(), &coin);
        }
    }

    let mut ledger_inputs = Vec::new();
    for (token_type, target_amount) in targets {
        let mut available = proposed
            .shielded
            .coins
            .iter()
            .filter(|(nullifier, coin)| {
                coin.type_ == token_type
                    && !proposed.shielded.pending_spends.contains_key(nullifier)
            })
            .map(|(_, coin)| *coin)
            .collect::<Vec<_>>();
        available.sort_by_key(|coin| coin.value);

        let mut selected_value = 0_u128;
        for coin in available {
            let (next, input) = proposed
                .shielded
                .spend(rng, &secret_keys, &coin, Some(0))
                .map_err(|_| MidnightRuntimeError::NativeInternal)?;
            proposed.shielded = next;
            ledger_inputs.push(input);
            selected_value = selected_value
                .checked_add(coin.value)
                .ok_or(MidnightRuntimeError::InvalidArgument)?;
            if selected_value >= target_amount {
                break;
            }
        }
        if selected_value < target_amount {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        let change = selected_value - target_amount;
        if change > 0 {
            let coin = ShieldedCoinInfo::new(rng, change, token_type);
            ledger_outputs.push(
                ZswapOutput::<ProofPreimage, InMemoryDB>::new(
                    rng,
                    &coin,
                    Some(0),
                    &secret_keys.coin_public_key(),
                    Some(secret_keys.enc_public_key()),
                )
                .map_err(|_| MidnightRuntimeError::NativeInternal)?,
            );
            proposed.shielded = proposed
                .shielded
                .watch_for(&secret_keys.coin_public_key(), &coin);
        }
    }

    proposed.refresh_coin_hashes(&secret_keys)?;
    ZswapOffer::new(ledger_inputs, ledger_outputs, Vec::new())
        .map(Some)
        .ok_or(MidnightRuntimeError::InvalidArgument)
}

fn build_unshielded_intent<R: Rng + CryptoRng>(
    proposed: &mut NativeWalletState,
    input: UnshieldedIntentInput<'_>,
    rng: &mut R,
) -> Result<
    Option<Intent<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB>>,
    MidnightRuntimeError,
> {
    let UnshieldedIntentInput {
        network_id,
        outputs,
        targets,
        night_external_key,
        ttl_seconds,
        fallible,
    } = input;
    if outputs.is_empty() && targets.is_empty() {
        return Ok(None);
    }
    let signing_key = SigningKey::from_bytes(night_external_key)
        .map_err(|_| MidnightRuntimeError::StateIncompatible)?;
    let owner = UserAddress::from(signing_key.verifying_key());
    let mut ledger_outputs = outputs
        .iter()
        .map(|output| {
            Ok(UtxoOutput {
                value: output.amount,
                owner: decode_unshielded_address(&output.receiver_address, network_id)?,
                type_: UnshieldedTokenType(decode_hash(&output.token_type)?),
            })
        })
        .collect::<Result<Vec<_>, MidnightRuntimeError>>()?;
    let mut ledger_inputs = Vec::new();
    let mut selected_coins = Vec::new();

    for (token_type, target_amount) in targets {
        let mut available = proposed
            .unshielded
            .available_utxos
            .iter()
            .filter_map(|coin| {
                let candidate_type = decode_hash(&coin.utxo.type_).ok()?;
                (candidate_type == token_type.0).then_some(coin.clone())
            })
            .collect::<Vec<_>>();
        available.sort_by_key(|coin| coin.utxo.value.parse::<u128>().unwrap_or(u128::MAX));
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
            ledger_inputs.push(UtxoSpend {
                value,
                owner: signing_key.verifying_key(),
                type_: token_type,
                intent_hash: IntentHash(decode_hash(&coin.utxo.intent_hash)?),
                output_no: coin.utxo.output_no,
            });
            selected_coins.push(coin);
            if selected_value >= target_amount {
                break;
            }
        }
        if selected_value < target_amount {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        let change = selected_value - target_amount;
        if change > 0 {
            ledger_outputs.push(UtxoOutput {
                value: change,
                owner,
                type_: token_type,
            });
        }
    }

    ledger_inputs.sort();
    ledger_outputs.sort();
    let offer = UnshieldedOffer::<Signature, InMemoryDB> {
        inputs: ledger_inputs.clone().into_iter().collect(),
        outputs: ledger_outputs.into_iter().collect(),
        signatures: Vec::new().into_iter().collect(),
    };
    let mut intent = Intent::<Signature, ProofPreimageMarker, PedersenRandomness, InMemoryDB>::new(
        rng,
        (!fallible).then_some(offer.clone()),
        fallible.then_some(offer),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
        midnight_base_crypto::time::Timestamp::from_secs(ttl_seconds),
    );
    let data_to_sign = intent.erase_proofs().erase_signatures().data_to_sign(1);
    let signature = signing_key.sign(rng, &data_to_sign);
    let target_offer = if fallible {
        &mut intent.fallible_unshielded_offer
    } else {
        &mut intent.guaranteed_unshielded_offer
    };
    let mut signed_offer = (**target_offer
        .as_ref()
        .ok_or(MidnightRuntimeError::NativeInternal)?)
    .clone();
    signed_offer.add_signatures(vec![signature; ledger_inputs.len()]);
    *target_offer = Some(Sp::new(signed_offer));

    for coin in selected_coins {
        remove_utxo(&mut proposed.unshielded.available_utxos, &coin.utxo);
        upsert_utxo(&mut proposed.unshielded.pending_utxos, coin);
    }
    Ok(Some(intent))
}

impl NativeWalletState {
    pub(crate) fn build_dapp_transfer_with_rng<R: Rng + CryptoRng>(
        &self,
        network_id: &str,
        outputs: &[DappTransactionOutput],
        night_external_key: &[u8],
        zswap_seed: &[u8],
        ttl_seconds: u64,
        rng: &mut R,
    ) -> Result<(Self, Vec<u8>), MidnightRuntimeError> {
        if network_id.is_empty() || outputs.is_empty() || outputs.len() > DAPP_MAX_COMPONENTS {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        let shielded_outputs = outputs
            .iter()
            .filter(|output| output.wallet_type == "shielded")
            .cloned()
            .collect::<Vec<_>>();
        let unshielded_outputs = outputs
            .iter()
            .filter(|output| output.wallet_type == "unshielded")
            .cloned()
            .collect::<Vec<_>>();
        if shielded_outputs.len() + unshielded_outputs.len() != outputs.len() {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        let shielded_targets = shielded_targets(
            shielded_outputs
                .iter()
                .map(|output| (output.token_type.clone(), output.amount)),
        )?;
        let unshielded_targets = unshielded_targets(
            unshielded_outputs
                .iter()
                .map(|output| (output.token_type.clone(), output.amount)),
        )?;
        let fallible = unshielded_targets
            .iter()
            .any(|(token_type, _)| token_type.0 == HashOutput::default());
        let mut proposed = self.clone();
        let shielded_offer = build_shielded_offer(
            &mut proposed,
            network_id,
            &shielded_outputs,
            shielded_targets,
            zswap_seed,
            rng,
        )?;
        let intent = build_unshielded_intent(
            &mut proposed,
            UnshieldedIntentInput {
                network_id,
                outputs: &unshielded_outputs,
                targets: unshielded_targets,
                night_external_key,
                ttl_seconds,
                fallible,
            },
            rng,
        )?;
        let intents = intent
            .map(|intent| [(1_u16, intent)].into_iter().collect())
            .unwrap_or_default();
        let transaction = Transaction::new(
            network_id.to_owned(),
            intents,
            shielded_offer,
            LedgerHashMap::new(),
        );
        let mut raw = Vec::new();
        tagged_serialize(&transaction, &mut raw)
            .map_err(|_| MidnightRuntimeError::NativeInternal)?;
        Ok((proposed, raw))
    }

    pub(crate) fn build_dapp_intent_with_rng<R: Rng + CryptoRng>(
        &self,
        input: DappIntentBuildInput<'_>,
        rng: &mut R,
    ) -> Result<Vec<u8>, MidnightRuntimeError> {
        let DappIntentBuildInput {
            network_id,
            inputs,
            outputs,
            night_external_key,
            zswap_seed,
            ttl_seconds,
        } = input;
        if network_id.is_empty()
            || inputs.is_empty()
            || inputs.len() > DAPP_MAX_COMPONENTS
            || outputs.len() > DAPP_MAX_COMPONENTS
            || outputs.iter().any(|output| output.amount == 0)
        {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        let shielded_inputs = inputs
            .iter()
            .filter(|input| input.wallet_type == "shielded")
            .cloned()
            .collect::<Vec<_>>();
        let unshielded_inputs = inputs
            .iter()
            .filter(|input| input.wallet_type == "unshielded")
            .cloned()
            .collect::<Vec<_>>();
        if shielded_inputs.len() + unshielded_inputs.len() != inputs.len() {
            return Err(MidnightRuntimeError::InvalidArgument);
        }
        let shielded_outputs = if shielded_inputs.is_empty() {
            Vec::new()
        } else {
            outputs
                .iter()
                .filter(|output| output.wallet_type == "shielded")
                .cloned()
                .collect()
        };
        let unshielded_outputs = if unshielded_inputs.is_empty() {
            Vec::new()
        } else {
            outputs
                .iter()
                .filter(|output| output.wallet_type == "unshielded")
                .cloned()
                .collect()
        };
        let shielded_targets = shielded_targets(
            shielded_inputs
                .into_iter()
                .map(|input| (input.token_type, input.amount)),
        )?;
        let unshielded_targets = unshielded_targets(
            unshielded_inputs
                .into_iter()
                .map(|input| (input.token_type, input.amount)),
        )?;
        let mut proposed = self.clone();
        let shielded_offer = build_shielded_offer(
            &mut proposed,
            network_id,
            &shielded_outputs,
            shielded_targets,
            zswap_seed,
            rng,
        )?;
        let intent = build_unshielded_intent(
            &mut proposed,
            UnshieldedIntentInput {
                network_id,
                outputs: &unshielded_outputs,
                targets: unshielded_targets,
                night_external_key,
                ttl_seconds,
                fallible: false,
            },
            rng,
        )?;
        let intents = intent
            .map(|intent| [(1_u16, intent)].into_iter().collect())
            .unwrap_or_default();
        let transaction = Transaction::new(
            network_id.to_owned(),
            intents,
            shielded_offer,
            LedgerHashMap::new(),
        );
        let mut raw = Vec::new();
        tagged_serialize(&transaction, &mut raw)
            .map_err(|_| MidnightRuntimeError::NativeInternal)?;
        Ok(raw)
    }
}
