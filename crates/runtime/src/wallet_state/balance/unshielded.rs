use super::*;

/// The wallet-side inputs and outputs that resolve one segment's unshielded
/// imbalance, plus the figures the approval screen has to show.
#[derive(Clone, Debug, Default)]
pub(super) struct UnshieldedPlan {
    pub(super) spent: Vec<UnshieldedUtxoWithMeta>,
    pub(super) inputs: Vec<UtxoSpend>,
    pub(super) outputs: Vec<UtxoOutput>,
    pub(super) contributions: BTreeMap<UnshieldedTokenType, u128>,
    pub(super) change: BTreeMap<UnshieldedTokenType, u128>,
}

impl UnshieldedPlan {
    fn is_empty(&self) -> bool {
        self.inputs.is_empty() && self.outputs.is_empty()
    }
}

fn parse_value(value: &str) -> Result<u128, MidnightRuntimeError> {
    value
        .parse::<u128>()
        .map_err(|_| MidnightRuntimeError::StateIncompatible)
}

/// Deterministic smallest-coin-first ordering, matching the Wallet SDK's coin
/// selection. The intent hash and output number break ties so two runs over the
/// same wallet state always select the same UTXOs.
fn candidates(
    state: &NativeWalletState,
    token: UnshieldedTokenType,
    already_taken: &[UnshieldedUtxoWithMeta],
) -> Result<Vec<UnshieldedUtxoWithMeta>, MidnightRuntimeError> {
    let mut available = Vec::new();
    for coin in &state.unshielded.available_utxos {
        if decode_hash(&coin.utxo.type_).ok() != Some(token.0) {
            continue;
        }
        if already_taken
            .iter()
            .any(|taken| same_utxo(&taken.utxo, &coin.utxo))
        {
            continue;
        }
        // A UTXO already registered as pending belongs to a transaction in
        // flight; spending it again would produce a transaction the node
        // rejects as a double spend.
        if state
            .unshielded
            .pending_utxos
            .iter()
            .any(|pending| same_utxo(&pending.utxo, &coin.utxo))
        {
            continue;
        }
        available.push(coin.clone());
    }
    let mut keyed = available
        .into_iter()
        .map(|coin| Ok((parse_value(&coin.utxo.value)?, coin)))
        .collect::<Result<Vec<_>, MidnightRuntimeError>>()?;
    keyed.sort_by(|left, right| {
        left.0.cmp(&right.0).then_with(|| {
            left.1
                .utxo
                .intent_hash
                .cmp(&right.1.utxo.intent_hash)
                .then_with(|| left.1.utxo.output_no.cmp(&right.1.utxo.output_no))
        })
    });
    Ok(keyed.into_iter().map(|(_, coin)| coin).collect())
}

/// Resolves every unshielded imbalance of one segment against the wallet.
///
/// A deficit draws wallet UTXOs and returns the remainder as change; a surplus
/// is taken back as a plain output. `already_taken` carries the UTXOs consumed
/// by earlier segments of the same plan so no coin is selected twice.
pub(super) fn plan_segment(
    state: &NativeWalletState,
    verifying_key: &VerifyingKey,
    owner: UserAddress,
    imbalances: &BTreeMap<UnshieldedTokenType, i128>,
    already_taken: &[UnshieldedUtxoWithMeta],
) -> Result<UnshieldedPlan, MidnightRuntimeError> {
    let mut plan = UnshieldedPlan::default();
    for (token, amount) in imbalances {
        if *amount > 0 {
            // The transaction over-supplies this token. Take the surplus back
            // rather than leaving the transaction unbalanced.
            let surplus =
                u128::try_from(*amount).map_err(|_| MidnightRuntimeError::InvalidArgument)?;
            plan.outputs.push(UtxoOutput {
                value: surplus,
                owner,
                type_: *token,
            });
            let entry = plan.change.entry(*token).or_default();
            *entry = entry.saturating_add(surplus);
            continue;
        }
        let needed = amount
            .checked_neg()
            .and_then(|value| u128::try_from(value).ok())
            .ok_or(MidnightRuntimeError::InvalidArgument)?;
        let mut taken = already_taken.to_vec();
        taken.extend(plan.spent.iter().cloned());
        let mut selected = 0_u128;
        for coin in candidates(state, *token, &taken)? {
            let value = parse_value(&coin.utxo.value)?;
            plan.inputs.push(UtxoSpend {
                value,
                owner: verifying_key.clone(),
                type_: *token,
                intent_hash: IntentHash(decode_hash(&coin.utxo.intent_hash)?),
                output_no: coin.utxo.output_no,
            });
            plan.spent.push(coin);
            selected = selected
                .checked_add(value)
                .ok_or(MidnightRuntimeError::InvalidArgument)?;
            if selected >= needed {
                break;
            }
        }
        if selected < needed {
            return Err(MidnightRuntimeError::InsufficientFunds);
        }
        let change = selected - needed;
        if change > 0 {
            plan.outputs.push(UtxoOutput {
                value: change,
                owner,
                type_: *token,
            });
            let entry = plan.change.entry(*token).or_default();
            *entry = entry.saturating_add(change);
        }
        let entry = plan.contributions.entry(*token).or_default();
        *entry = entry.saturating_add(needed);
    }
    Ok(plan)
}

/// One offer's inputs paired with the signature that already covers them.
///
/// A `None` signature marks an input this wallet added and must sign itself;
/// `Some` preserves a counterparty's existing signature through the re-sort
/// that adding inputs forces.
type SignedInputs = Vec<(UtxoSpend, Option<Signature>)>;

fn merge_offer(
    existing: Option<&UnshieldedOffer<Signature, InMemoryDB>>,
    added_inputs: &[UtxoSpend],
    added_outputs: &[UtxoOutput],
) -> (SignedInputs, Vec<UtxoOutput>) {
    let mut inputs: SignedInputs = Vec::new();
    let mut outputs = Vec::new();
    if let Some(existing) = existing {
        let signatures = Vec::from(&existing.signatures);
        for (index, input) in Vec::from(&existing.inputs).into_iter().enumerate() {
            inputs.push((input, signatures.get(index).cloned()));
        }
        outputs.extend(Vec::from(&existing.outputs));
    }
    inputs.extend(added_inputs.iter().cloned().map(|input| (input, None)));
    outputs.extend(added_outputs.iter().cloned());
    inputs.sort_by(|left, right| left.0.cmp(&right.0));
    outputs.sort();
    (inputs, outputs)
}

/// Installs an offer whose signatures are still missing for the wallet's own
/// inputs. Signature data is derived from the signature-erased intent, so an
/// offer can be installed first and signed once the intent is final.
fn install(
    inputs: &SignedInputs,
    outputs: Vec<UtxoOutput>,
) -> UnshieldedOffer<Signature, InMemoryDB> {
    UnshieldedOffer {
        inputs: inputs
            .iter()
            .map(|(input, _)| input.clone())
            .collect::<Vec<_>>()
            .into_iter()
            .collect(),
        outputs: outputs.into_iter().collect(),
        signatures: Vec::new().into_iter().collect(),
    }
}

fn signed(inputs: &SignedInputs, signature: &Signature) -> Vec<Signature> {
    inputs
        .iter()
        .map(|(_, existing)| existing.clone().unwrap_or_else(|| signature.clone()))
        .collect()
}

/// The edits one intent needs before it can be signed.
#[derive(Default)]
pub(super) struct IntentEdit {
    guaranteed: Option<(SignedInputs, Vec<UtxoOutput>)>,
    fallible: Option<(SignedInputs, Vec<UtxoOutput>)>,
}

impl IntentEdit {
    pub(super) fn stage(
        &mut self,
        intent: &UnsealedIntent,
        guaranteed_section: bool,
        plan: &UnshieldedPlan,
    ) {
        let existing = if guaranteed_section {
            intent.guaranteed_unshielded_offer.as_deref()
        } else {
            intent.fallible_unshielded_offer.as_deref()
        };
        let merged = merge_offer(existing, &plan.inputs, &plan.outputs);
        if guaranteed_section {
            self.guaranteed = Some(merged);
        } else {
            self.fallible = Some(merged);
        }
    }

    /// Rewrites the intent with the staged offers and signs every input the
    /// wallet added, leaving counterparty signatures untouched.
    pub(super) fn apply<R: Rng + CryptoRng>(
        &self,
        intent: &UnsealedIntent,
        segment: u16,
        signing_key: &SigningKey,
        rng: &mut R,
    ) -> Result<UnsealedIntent, MidnightRuntimeError> {
        let mut updated = intent.clone();
        if let Some((inputs, outputs)) = &self.guaranteed {
            updated.guaranteed_unshielded_offer = Some(Sp::new(install(inputs, outputs.clone())));
        }
        if let Some((inputs, outputs)) = &self.fallible {
            updated.fallible_unshielded_offer = Some(Sp::new(install(inputs, outputs.clone())));
        }
        let data_to_sign = updated
            .erase_proofs()
            .erase_signatures()
            .data_to_sign(segment);
        let signature = signing_key.sign(rng, &data_to_sign);
        if let Some((inputs, _)) = &self.guaranteed {
            let mut offer = (*updated
                .guaranteed_unshielded_offer
                .as_ref()
                .ok_or(MidnightRuntimeError::NativeInternal)?
                .clone())
            .clone();
            offer.add_signatures(signed(inputs, &signature));
            updated.guaranteed_unshielded_offer = Some(Sp::new(offer));
        }
        if let Some((inputs, _)) = &self.fallible {
            let mut offer = (*updated
                .fallible_unshielded_offer
                .as_ref()
                .ok_or(MidnightRuntimeError::NativeInternal)?
                .clone())
            .clone();
            offer.add_signatures(signed(inputs, &signature));
            updated.fallible_unshielded_offer = Some(Sp::new(offer));
        }
        Ok(updated)
    }
}

/// Builds the standalone guaranteed-section intent that balances a sealed
/// transaction. Sealed intents cannot be edited, so the wallet's inputs live in
/// a separate intent that is merged in after proving.
pub(super) fn balancing_intent<R: Rng + CryptoRng>(
    plan: &UnshieldedPlan,
    ttl_seconds: u64,
    rng: &mut R,
) -> Result<Option<UnprovenIntent>, MidnightRuntimeError> {
    if plan.is_empty() {
        return Ok(None);
    }
    let (inputs, outputs) = merge_offer(None, &plan.inputs, &plan.outputs);
    // The segment identifier is only known once the intent is placed in the
    // balancing transaction, and `data_to_sign` depends on it, so signing is
    // deferred to `sign_balancing_intent`.
    Ok(Some(Intent::<
        Signature,
        ProofPreimageMarker,
        PedersenRandomness,
        InMemoryDB,
    >::new(
        rng,
        Some(install(&inputs, outputs)),
        None,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
        midnight_base_crypto::time::Timestamp::from_secs(ttl_seconds),
    )))
}

pub(super) fn sign_balancing_intent<R: Rng + CryptoRng>(
    intent: &UnprovenIntent,
    segment: u16,
    signing_key: &SigningKey,
    rng: &mut R,
) -> Result<UnprovenIntent, MidnightRuntimeError> {
    let mut updated = intent.clone();
    let data_to_sign = updated
        .erase_proofs()
        .erase_signatures()
        .data_to_sign(segment);
    let signature = signing_key.sign(rng, &data_to_sign);
    let mut offer = (**updated
        .guaranteed_unshielded_offer
        .as_ref()
        .ok_or(MidnightRuntimeError::NativeInternal)?)
    .clone();
    let count = offer.inputs.len();
    offer.add_signatures(vec![signature; count]);
    updated.guaranteed_unshielded_offer = Some(Sp::new(offer));
    Ok(updated)
}

/// Moves the UTXOs a plan spends from available to pending in a cloned wallet
/// state. Balance-only preparation drops that clone, so a preview never
/// reserves a coin.
pub(super) fn reserve(state: &mut NativeWalletState, spent: &[UnshieldedUtxoWithMeta]) {
    for coin in spent {
        remove_utxo(&mut state.unshielded.available_utxos, &coin.utxo);
        upsert_utxo(&mut state.unshielded.pending_utxos, coin.clone());
    }
}
